use std::cmp::{max, min};

use sqlx::PgPool;
use sqlx::types::Uuid;

use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

use crate::bot::{handle_error, log_try_remove_markup, match_callback_data, retrieve_callback_data, send_msg, send_or_edit_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit, notifier, utils};
use crate::types::{Availability, AvailabilityDetails, Usr};

use serde::{Serialize, Deserialize};
use strum::EnumProperty;
use callback_data::CallbackData;
use callback_data::CallbackDataHandler;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum Saf100ViewType {
    SeeAvail,
    SeePlanned
}

// Represents callback actions with optional associated data.
#[derive(Debug, Clone, Serialize, Deserialize, EnumProperty, CallbackData)]
enum Saf100CallbackData {
    // Initial Selection Actions
    See { view_type: Saf100ViewType},
    Cancel,

    // Pagination Actions
    Prev,
    Next,
    Done,

    // Actions with associated UUID
    Select { id: Uuid },
    ConfirmYes,
    ConfirmNo,
}

fn get_paginated_keyboard(
    availability: &Vec<AvailabilityDetails>,
    prefix: &String,
    start: usize,
    show: usize
) -> Result<InlineKeyboardMarkup, ()> {
    let slice_end = min(start + show, availability.len());
    let shown_entries = match availability.get(start..slice_end) {
        Some(entries) => entries,
        None => {
            log::error!("Cannot get availability entries slice");
            return Err(());
        }
    };

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries
        .iter()
        .filter_map(|entry| {
            if !entry.saf100 && entry.is_valid {
                // Format date as "MMM-DD" (3-letter month)
                let formatted = format!(
                    "{}: {} {}",
                    entry.ops_name,
                    entry.avail.format("%b-%d"),
                    entry.ict_type.as_ref()
                );

                Some(InlineKeyboardButton::callback(
                    formatted,
                    Saf100CallbackData::Select { id: entry.id }.to_callback_data(prefix),
                ))
            } else { None }
        })
        .map(|button| vec![button])
        .collect();

    // Add "PREV", "NEXT", and "DONE" buttons
    let mut pagination = Vec::new();
    if start > 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", Saf100CallbackData::Prev.to_callback_data(prefix)));
    }
    if slice_end < availability.len() {
        pagination.push(InlineKeyboardButton::callback("NEXT", Saf100CallbackData::Next.to_callback_data(prefix)));
    }
    pagination.push(InlineKeyboardButton::callback("DONE", Saf100CallbackData::Done.to_callback_data(prefix)));

    // Combine entries with pagination
    entries.push(pagination);

    Ok(InlineKeyboardMarkup::new(entries))
}

fn get_paginated_text(
    availability: &Vec<AvailabilityDetails>,
    start: usize,
    show: usize,
    action: &Saf100ViewType,
) -> String {
    let slice_end = min(start + show, availability.len());
    let shown_entries = availability.get(start..slice_end).unwrap_or(&[]);

    // Header
    let header = format!(
        "Showing {}availability \\({} to {}\\) out of {}\\.\n\n",
        match action {
            Saf100ViewType::SeeAvail => { "" }
            Saf100ViewType::SeePlanned => { "*planned* " }
        },
        start + 1,
        slice_end,
        availability.len()
    );

    // Calculate the length of the longest `ops_name`
    let max_len = availability.iter()
        .map(|info| info.ops_name.len())
        .max()
        .unwrap_or(0); // Handle case when result is empty

    // Prepare the list of availability entries with SAF100 status
    let mut count = 0;
    let mut entries_text = String::new();
    for entry in shown_entries {
        let saf100_status = if entry.saf100 { "✅ SAF100 Issued" } else { if entry.planned { "❌ SAF100 Pending" } else { " \\(NOT PLANNED\\)" }};
        let avail_status = if entry.is_valid { "" } else { " *\\(NOT AVAIL\\)*" };
        entries_text.push_str(format!(
            "\\- `{:<width$}`: {} {}\n{}{}\n\n",
            utils::escape_special_characters(&entry.ops_name),
            utils::escape_special_characters(&entry.avail.format("%b-%d").to_string()),
            utils::escape_special_characters(&entry.ict_type.as_ref()),
            saf100_status, avail_status,
            width = max_len // Dynamically set the width
        ).as_str());
        
        if entry.is_valid && !entry.saf100 { count += 1; }
    }

    // Footer with instructions
    let footer = if availability.is_empty() || count == 0 { "\nNo pending SAF100 to issue\\.".to_string() } else { "\nWhich entry do you want to confirm issued SAF100?".to_string() };

    // Combine all parts
    format!("{}{}{}", header, entries_text, footer)
}

// Function to update the original paginated message
async fn update_paginated_message(
    bot: &Bot,
    chat_id: ChatId,
    prefix: &String,
    availability_list: &Vec<AvailabilityDetails>,
    new_start: &usize,
    show: &usize,
    action: &Saf100ViewType,
    msg_id: Option<MessageId>,
    username: &Option<String>,
    no_button: bool
) -> Result<Option<MessageId>, ()> {
    // Generate the paginated keyboard
    let paginated_keyboard = match get_paginated_keyboard(availability_list, prefix, *new_start, *show) {
        Ok(kb) => kb,
        Err(_) => {
            send_msg(
                bot.send_message(chat_id, "Error encountered while getting availability."),
                &username,
            ).await;
            return Err(());
        }
    };

    let message_text = get_paginated_text(availability_list, *new_start, *show, action);
    // Send or edit the message
    Ok(send_or_edit_msg(bot, chat_id, username, msg_id, message_text, if !no_button { Some(paginated_keyboard) } else { None }, Some(ParseMode::MarkdownV2)).await)
}

async fn handle_re_show_options(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    start: usize,
    show: usize,
    action: Saf100ViewType,
    msg_id: Option<MessageId>,
    pool: &PgPool
) -> HandlerResult {

    // Fetch the relevant availability entries
    let availability_result = match action {
        Saf100ViewType::SeeAvail => {
            controllers::attendance::get_future_valid_availability_for_ns(pool).await
        }
        Saf100ViewType::SeePlanned => {
            controllers::attendance::get_future_planned_availability_for_ns(pool).await
        }
    };

    match availability_result {
        Ok(availability_list) => {
            if availability_list.is_empty() {
                // No entries found
                send_or_edit_msg(&bot, dialogue.chat_id(), &username, msg_id, "No availability entries found.".into(), None, None).await;
                dialogue.update(State::Start).await?;
                return Ok(());
            }

            // Generate a random prefix for callback data
            let prefix: String = utils::generate_prefix(utils::CALLBACK_PREFIX_LEN);

            let entries: Vec<bool> = availability_list.iter().filter_map(|entry| { if !entry.saf100 && entry.is_valid { Some(true) } else { None } }).collect();
            let no_buttons = entries.len() <= 0;
            let new_start = if start >= availability_list.len() { max(0, availability_list.len() as i64 - show as i64) as usize } else { start };
            match update_paginated_message(&bot, dialogue.chat_id(), &prefix, &availability_list, &new_start, &show, &action, msg_id, username, no_buttons)
                .await {
                Ok(msg_id) => {
                    match msg_id {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => {
                            if no_buttons {
                                dialogue.update(State::Start).await?
                            } else {
                                // Update the state with the new start index
                                dialogue.update(State::Saf100View { msg_id: new_msg_id, availability_list, prefix, start: new_start, action }).await?
                            }
                        }
                    }
                }
                Err(_) => {
                    log::error!("Failed to update saf100 paginated menu in chat ({})", dialogue.chat_id().0);
                    dialogue.update(State::ErrorState).await?
                }
            };
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), username).await
    }

    Ok(())
}

fn get_confirmation_keyboard(prefix: &String) -> InlineKeyboardMarkup {
    let confirm = [("YES", Saf100CallbackData::ConfirmYes), ("NO", Saf100CallbackData::ConfirmNo)]
        .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(&prefix)));
    InlineKeyboardMarkup::new([confirm])
}

pub(super) async fn saf100(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "saf100", "Command", msg);

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Generate a random prefix for callback data
    let prefix: String = utils::generate_prefix(utils::CALLBACK_PREFIX_LEN);

    // Define the inline keyboard buttons
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("SEE AVAIL", Saf100CallbackData::See { view_type: Saf100ViewType::SeeAvail }.to_callback_data(&prefix)),
            InlineKeyboardButton::callback("SEE PLANNED", Saf100CallbackData::See { view_type: Saf100ViewType::SeePlanned }.to_callback_data(&prefix)), ],
        vec![InlineKeyboardButton::callback("CANCEL", Saf100CallbackData::Cancel.to_callback_data(&prefix)), ],
    ]);

    let msg_id = match send_msg(
        bot.send_message(dialogue.chat_id(), "Please choose an option:")
            .reply_markup(keyboard),
        &user.username
    ).await {
        None => {
            log::error!("Failed to send saf100 options message");
            dialogue.update(State::ErrorState).await?;
            return Ok(());
        }
        Some(msg_id) => msg_id
    };

    // Update the dialogue state to Saf100Select with the original message ID
    dialogue.update(State::Saf100Select { msg_id, prefix }).await?;

    Ok(())
}

pub(super) async fn saf100_select(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, prefix): (MessageId, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "saf100_select", "Callback", q,
        "MessageId" => msg_id,
        "Prefix" => prefix
    );

    // Extract the callback data
    let data = match retrieve_callback_data(&bot, dialogue.chat_id(), &q).await {
        Ok(data) => data,
        Err(_) => { return Ok(()); }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    // Deserialize the callback data into the enum
    let callback = match match_callback_data(&bot, dialogue.chat_id(), &q.from.username, &data, &prefix).await {
        Ok(callback) => callback,
        Err(_) => { return Ok(()); }
    };

    // Handle based on the variant
    match callback {
        Saf100CallbackData::See { view_type } => {
            handle_re_show_options(&bot, &dialogue, &q.from.username, 0, utils::MAX_SHOW_ENTRIES, view_type, Some(msg_id), &pool).await?;
        }
        Saf100CallbackData::Cancel => {
            // Handle cancellation by reverting to the start state
            send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), "Operation cancelled.".into(), None, None).await;
            dialogue.update(State::Start).await?;
        }
        _ => {
            // Handle unexpected actions
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn saf100_view(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability_list, prefix, start, action): (
        MessageId,
        Vec<AvailabilityDetails>,
        String,
        usize,
        Saf100ViewType,
    ),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "saf100_view", "Callback", q,
        "MessageId" => msg_id,
        "AvailabilityList" => availability_list,
        "Prefix" => prefix,
        "Start" => start,
        "Action" => action
    );

    // Extract the callback data
    let data = match retrieve_callback_data(&bot, dialogue.chat_id(), &q).await {
        Ok(data) => data,
        Err(_) => { return Ok(()); }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    // Deserialize the callback data into the enum
    let callback = match match_callback_data(&bot, dialogue.chat_id(), &q.from.username, &data, &prefix).await {
        Ok(callback) => callback,
        Err(_) => { return Ok(()); }
    };

    let show = utils::MAX_SHOW_ENTRIES;

    match callback {
        Saf100CallbackData::Prev => {
            let new_start = if start >= show { start - show } else { 0 };
            handle_re_show_options(&bot, &dialogue, &q.from.username, new_start, show, action, Some(msg_id), &pool).await?;
        }
        Saf100CallbackData::Next => {
            let entries_len = availability_list.len();
            let new_start = if start + show < entries_len { start + show } else { start };
            handle_re_show_options(&bot, &dialogue, &q.from.username, new_start, show, action, Some(msg_id), &pool).await?;
        }
        Saf100CallbackData::Done => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, None, "Operation completed.".into(), None, None).await;
            dialogue.update(State::Start).await?
        }
        Saf100CallbackData::Select { id: parsed_id} => {
            // Handle selection of an availability entry
            match controllers::scheduling::get_availability_by_uuid(&pool, parsed_id).await {
                Ok(availability_entry) => {
                    // Handle the selected availability entry as needed
                    // For example, display details or allow modifications
                    let details_text = format!(
                        "Selected Availability:\nDate: {}\nType: {}\nRemarks: {}\n\nConfirm issued SAF100?",
                        availability_entry.avail.format("%Y-%m-%d"),
                        availability_entry.ict_type.as_ref(),
                        availability_entry.remarks.as_deref().unwrap_or("None")
                    );

                    let new_prefix = utils::generate_prefix(utils::CALLBACK_PREFIX_LEN);

                    // Send or edit message
                    match send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), details_text, Some(get_confirmation_keyboard(&new_prefix)), None).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => dialogue.update(State::Saf100Confirm { msg_id: new_msg_id, availability: availability_entry, prefix: new_prefix, start, action }).await?
                    }
                }
                Err(e) => {
                    log::error!("Error fetching availability by UUID: {}", e);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Failed to retrieve availability details."),
                        &q.from.username,
                    ).await;
                }
            }

        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn saf100_confirm(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability, prefix, start, action): (MessageId, Availability, String, usize, Saf100ViewType),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "saf100_confirm", "Callback", q,
        "MessageId" => msg_id,
        "Availability" => availability,
        "Prefix" => prefix,
        "Start" => start,
        "Action" => action
    );

    // Extract the callback data
    let data = match retrieve_callback_data(&bot, dialogue.chat_id(), &q).await {
        Ok(data) => data,
        Err(_) => { return Ok(()); }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    // Deserialize the callback data into the enum
    let callback = match match_callback_data(&bot, dialogue.chat_id(), &q.from.username, &data, &prefix).await {
        Ok(callback) => callback,
        Err(_) => { return Ok(()); }
    };

    match callback {
        Saf100CallbackData::ConfirmYes => {
            match controllers::attendance::set_saf100_true_by_uuid(&pool, availability.id).await {
                Ok(details) => {
                    notifier::emit::plan_notifications(
                        &bot,
                        format!(
                            "{} has confirmed SAF100 issued for `{}` on {}",
                             utils::username_link_tag(&q.from),
                            utils::escape_special_characters(&details.ops_name),
                            utils::escape_special_characters(&details.avail.format("%Y-%m-%d").to_string())
                        ).as_str(),
                        &pool,
                        q.from.id.0 as i64
                    ).await;
                    
                    let failed;
                    let mut retrieved: Option<Usr> = None;
                    
                    match controllers::user::get_user_by_uuid(&pool, availability.user_id).await {
                        Ok(user_details) => {
                            // Notify the user that SAF100 has been sent for them
                            match send_msg(
                                bot.send_message(
                                    ChatId(user_details.tele_id), 
                                    format!("SAF100 for {} sent to NS branch for processing.\nPlease look out for it.", details.avail.format("%Y-%m-%d"))
                                ),
                                &q.from.username,
                            ).await {
                                None => failed = true,
                                Some(_) => failed = false,
                            }
                            retrieved = Some(user_details);
                        }
                        Err(_) => {
                            // Failed to notify nsmen
                            failed = true;
                        }
                    }
                    
                    let mut message_text = format!(
                        "SAF100 confirmed issued for `{}` on {}", 
                        utils::escape_special_characters(&details.ops_name), 
                        utils::escape_special_characters(&details.avail.format("%Y-%m-%d").to_string())
                    );
                    
                    if failed {
                        message_text.push_str("\n⚠️ *Failed* to send notification to ");
                        match retrieved {
                            None => {}
                            Some(details) => {
                                // Fetch the user's chat info dynamically using getChat
                                let user_chat = bot.get_chat(ChatId(details.tele_id)).await;
                                let mut failed_to_get_username = false;
                                match user_chat {
                                    Ok(chat) => {
                                        match chat.username() {
                                            None => { failed_to_get_username = true; }
                                            Some(username) => {
                                                message_text.push_str(format!(" @{} \\!", utils::escape_special_characters(&username)).as_str());
                                            }
                                        }
                                    },
                                    Err(_) => { failed_to_get_username = true; }
                                };
                                if failed_to_get_username {
                                    message_text.push_str(format!("[{}](tg://user?id={}) \\!", utils::escape_special_characters(&details.name), details.tele_id as u64).as_str())
                                }
                            }
                        }
                    }
                    
                    // Send or edit message
                    send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), message_text, None, Some(ParseMode::MarkdownV2)).await;
                    handle_re_show_options(&bot, &dialogue, &q.from.username, start, utils::MAX_SHOW_ENTRIES, action, None, &pool).await?;
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await,
            }
        }
        Saf100CallbackData::ConfirmNo => {
            // logic to go back
            handle_re_show_options(&bot, &dialogue, &q.from.username, start, utils::MAX_SHOW_ENTRIES, action, Some(msg_id), &pool).await?;
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
    }
    
    Ok(())
}