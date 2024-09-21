use std::cmp::min;
use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::PgPool;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId};
use sqlx::types::Uuid;
use crate::bot::{handle_error, send_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit, notifier};
use crate::types::{Availability, AvailabilityDetails};

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
        .map(|entry| {
            // Format date as "MMM-DD" (3-letter month)
            let formatted = format!(
                "{}: {} {}",
                entry.ops_name,
                entry.avail.format("%b-%d"),
                entry.ict_type.as_ref()
            );

            InlineKeyboardButton::callback(
                formatted,
                format!("{}{}", prefix, entry.id),
            )
        })
        .map(|button| vec![button])
        .collect();

    // Add "PREV", "NEXT", and "DONE" buttons
    let mut pagination = Vec::new();
    if start > 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", "PREV"));
    }
    if slice_end < availability.len() {
        pagination.push(InlineKeyboardButton::callback("NEXT", "NEXT"));
    }
    pagination.push(InlineKeyboardButton::callback("DONE", "DONE"));

    // Combine entries with pagination
    entries.push(pagination);

    Ok(InlineKeyboardMarkup::new(entries))
}

fn get_paginated_text(
    availability: &Vec<AvailabilityDetails>,
    start: usize,
    show: usize,
    action: &String,
) -> String {
    // Prepare the message text
    let slice_end = min(start + show, availability.len());
    let total = availability.len();
    format!(
        "Showing {}availability {} to {} out of {}. Choose one to view details:",
        if action == "SAF100_SEE_AVAIL" { "" } else { "planned "},
        start + 1,
        slice_end,
        total
    )
}

// Function to update the original paginated message
async fn update_paginated_message(
    bot: &Bot,
    chat_id: ChatId,
    prefix: &String,
    availability_list: &Vec<AvailabilityDetails>,
    new_start: &usize,
    show: &usize,
    action: &String,
    msg_id: &MessageId,
    username: &Option<String>,
) -> Result<(), ()> {
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
    // Edit the existing message
    if let Err(e) = bot.edit_message_text(chat_id, *msg_id, message_text)
        .reply_markup(paginated_keyboard).await {
        log::error!("Failed to edit saf100 message during pagination: {}", e);
        send_msg(
            bot.send_message(chat_id, "Failed to update availability view."),
            &username,
        ).await;
        return Err(());
    }

    Ok(())
}

async fn handle_re_show_options(
    bot: &Bot,
    dialogue: &MyDialogue,
    prefix: String,
    availability_list: Vec<AvailabilityDetails>,
    start: usize,
    show: usize,
    action: String,
    msg_id: MessageId,
    username: &Option<String>,
) -> HandlerResult {
    match update_paginated_message(&bot, dialogue.chat_id(), &prefix, &availability_list, &start, &show, &action, &msg_id, username)
        .await {
        Ok(_) => {
            // Update the state with the new start index
            dialogue.update(State::Saf100View { msg_id, availability_list, prefix, start, action }).await?
        }
        Err(_) => dialogue.update(State::ErrorState).await?
    };
    Ok(())
}

fn get_confirmation_keyboard() -> InlineKeyboardMarkup {
    let confirm = ["YES", "NO"]
        .map(|product| InlineKeyboardButton::callback(product, product));
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

    // Define the inline keyboard buttons
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("SEE AVAIL", "SAF100_SEE_AVAIL"),
            InlineKeyboardButton::callback("SEE PLANNED", "SAF100_SEE_PLANNED"), ],
        vec![InlineKeyboardButton::callback("CANCEL", "SAF100_CANCEL"), ],
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
    dialogue.update(State::Saf100Select { msg_id }).await?;

    Ok(())
}

pub(super) async fn saf100_select(
    bot: Bot,
    dialogue: MyDialogue,
    msg_id: MessageId,
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "saf100_select", "Callback", q,
        "State" => "Saf100Select"
    );

    // Extract the callback data
    let data = match q.data.as_ref() {
        Some(d) => d.clone(),
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            dialogue.update(State::Saf100Select { msg_id }).await?;
            return Ok(());
        }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match data.as_str() {
        "SAF100_SEE_AVAIL" | "SAF100_SEE_PLANNED" => {
            // Determine the action based on selection
            let action = data.clone();

            // Fetch the relevant availability entries
            let availability_result = if action == "SAF100_SEE_AVAIL" {
                controllers::attendance::get_future_valid_availability_for_ns(&pool).await
            } else {
                controllers::attendance::get_future_planned_availability_for_ns(&pool).await
            };

            match availability_result {
                Ok(availability_list) => {
                    if availability_list.is_empty() {
                        // No entries found
                        match bot.edit_message_text(dialogue.chat_id(), msg_id, "No availability entries found.").await {
                            Ok(_) => {}
                            Err(_) => {
                                // Failed to edit the message; consider sending a new one or logging
                                log::error!("Failed to edit message to show no entries.");
                            }
                        };
                        dialogue.update(State::Start).await?;
                        return Ok(());
                    }

                    // Generate a random prefix for callback data
                    let prefix: String = rand::thread_rng()
                        .sample_iter(&Alphanumeric)
                        .take(5)
                        .map(char::from)
                        .collect();

                    let start = 0;
                    let show = 8;

                    // Generate the paginated keyboard
                    let paginated_keyboard = match get_paginated_keyboard(&availability_list, &prefix, start, show) {
                        Ok(kb) => kb,
                        Err(_) => {
                            send_msg(
                                bot.send_message(dialogue.chat_id(), "Error encountered while getting availability."),
                                &q.from.username,
                            ).await;
                            dialogue.update(State::ErrorState).await?;
                            return Ok(());
                        }
                    };

                    // Generate the message text
                    let message_text = get_paginated_text(&availability_list, start, show, &action);

                    // Edit the original message with new text and keyboard
                    match bot.edit_message_text(dialogue.chat_id(), msg_id, message_text)
                        .reply_markup(paginated_keyboard).await {
                        Ok(_) => {
                            // Update the dialogue state to Saf100View with necessary context
                            dialogue.update(State::Saf100View { msg_id, availability_list, prefix, start, action }).await?;
                        }
                        Err(e) => {
                            log::error!("Failed to edit saf100 message: {}", e);
                            send_msg(
                                bot.send_message(dialogue.chat_id(), "Failed to update availability view."),
                                &q.from.username,
                            ).await;
                            dialogue.update(State::ErrorState).await?;
                        }
                    };
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
            }
        }
        "SAF100_CANCEL" => {
            // Handle cancellation by reverting to the start state
            match bot.edit_message_text(dialogue.chat_id(), msg_id, "Operation cancelled.").await {
                Ok(_) => {}
                Err(e) => {
                    // Failed to edit the message; consider sending a new one or logging
                    log::error!("Failed to edit saf100 message on cancel: {}", e);
                    dialogue.update(State::ErrorState).await?;
                }
            };
            
            dialogue.update(State::Start).await?;
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

pub(super) async fn saf100_view(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability_list, prefix, start, action): (
        MessageId,
        Vec<AvailabilityDetails>,
        String,
        usize,
        String,
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
    let data = match q.data.as_ref() {
        Some(d) => d.clone(),
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            return Ok(());
        }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }
    let show = 8;

    match data.as_str() {
        "PREV" => {
            let new_start = if start >= show { start - show } else { 0 };
            handle_re_show_options(&bot, &dialogue, prefix, availability_list, new_start, show, action, msg_id, &q.from.username).await?;
        }
        "NEXT" => {
            let entries_len = availability_list.len();
            let new_start = if start + show < entries_len { start + show } else { start };
            handle_re_show_options(&bot, &dialogue, prefix, availability_list, new_start, show, action, msg_id, &q.from.username).await?;
        }
        "DONE" => {
            match bot.edit_message_text(dialogue.chat_id(), msg_id, "Operation completed.")
                .await {
                Ok(_) => dialogue.update(State::Start).await?,
                Err(e) => {
                    log::error!("Failed to edit saf100 message on DONE: {}", e);
                    dialogue.update(State::ErrorState).await?
                }
            };
        }
        _ => {
            // Handle selection of an availability entry
            if data.starts_with(&prefix) {
                let id_str = data.strip_prefix(&prefix).unwrap_or("");
                match Uuid::parse_str(id_str) {
                    Ok(parsed_id) => {
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

                                match bot.edit_message_text(dialogue.chat_id(), msg_id, details_text)
                                    .reply_markup(get_confirmation_keyboard())
                                    .await {
                                    Ok(_) => {
                                        dialogue.update(State::Saf100Confirm { msg_id, availability: availability_entry, availability_list, prefix, start, action }).await?
                                    },
                                    Err(e) => {
                                        log::error!("Failed to edit saf100 message with details: {}", e);
                                        send_msg(
                                            bot.send_message(dialogue.chat_id(), "Failed to display availability details."),
                                            &q.from.username,
                                        ).await;
                                        dialogue.update(State::ErrorState).await?
                                    }
                                };
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
                    Err(_) => {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "Invalid availability selection."),
                            &q.from.username,
                        ).await;
                    }
                }
            } else {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Invalid option."),
                    &q.from.username,
                ).await;
            }
        }
    }

    Ok(())
}

pub(super) async fn saf100_confirm(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability, availability_list, prefix, start, action): (
        MessageId,
        Availability,
        Vec<AvailabilityDetails>,
        String,
        usize,
        String,
    ),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "saf100_confirm", "Callback", q,
        "MessageId" => msg_id,
        "Availability" => availability,
        "AvailabilityList" => availability_list,
        "Prefix" => prefix,
        "Start" => start,
        "Action" => action
    );

    // Extract the callback data
    let data = match q.data.as_ref() {
        Some(d) => d.clone(),
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            return Ok(());
        }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match data.as_str() {
        "YES" => {
            match controllers::attendance::set_saf100_true_by_uuid(&pool, availability.id).await {
                Ok(details) => {
                    notifier::emit::system_notifications(
                        &bot,
                        format!(
                            "User @{} has confirmed SAF100 issued for {} on {}",
                            &q.from.username.as_deref().unwrap_or("none"), details.ops_name, details.avail.format("%Y-%m-%d")
                        ).as_str(),
                        &pool,
                        q.from.id.0 as i64
                    ).await;
                    
                    match bot.edit_message_text(
                        dialogue.chat_id(), msg_id, 
                        format!("SAF100 confirmed issued for {} on {}", details.ops_name, details.avail.format("%Y-%m-%d"))
                    ).await {
                        Ok(_) => {},
                        Err(e) => {
                            log::error!("Failed to edit saf100 message with details: {}", e);
                            send_msg(
                                bot.send_message(dialogue.chat_id(), "Updated"),
                                &q.from.username,
                            ).await;
                        }
                    };
                    dialogue.update(State::Start).await?;
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await,
            }
        }
        "NO" => {
            // logic to go back
            handle_re_show_options(&bot, &dialogue, prefix, availability_list, start, 8, action, msg_id, &q.from.username).await?;
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