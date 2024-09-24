use crate::bot::state::State;
use crate::bot::{handle_error, log_try_delete_msg, log_try_remove_markup, send_msg, send_or_edit_msg, HandlerResult, MyDialogue};
use crate::types::{Availability, AvailabilityDetails, Ict, UsrType};
use crate::{controllers, log_endpoint_hit, notifier, utils};
use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::types::chrono::NaiveDate;
use sqlx::types::Uuid;
use sqlx::PgPool;
use std::cmp::{max, min};
use std::str::FromStr;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

fn get_availability_edit_keyboard(
    availability: &Vec<Availability>,
    prefix: &String,
    start: usize,
    show: usize,
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
            let truncated_remarks = if let Some(remarks) = &entry.remarks {
                if remarks.chars().count() > 8 {
                    format!(", {}...", remarks.chars().take(8).collect::<String>())
                } else {
                    format!(", {}", remarks)
                }
            } else {
                "".to_string()
            };

            let is_planned_str = if entry.planned { " (PLAN) " } else { "" };

            // Format date as "MMM-DD" (3-letter month)
            let formatted = format!(
                "{}: {}{}{}",
                entry.avail.format("%b-%d"),
                is_planned_str,
                entry.ict_type.as_ref(),
                truncated_remarks
            );

            if entry.is_valid {
                Some(vec![InlineKeyboardButton::callback(
                    formatted,
                    format!("{}{}", prefix, entry.id),
                )])
            } else {
                None
            }
        })
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

fn get_availability_edit_text(
    availability: &Vec<Availability>,
    start: usize,
    show: usize,
    action: &String,
) -> String {
    // Prepare the message text
    let slice_end = min(start + show, availability.len());
    let message_text = format!(
        "Showing availability {} to {}, choose one to {}",
        start + 1,
        slice_end,
        action.to_lowercase()
    );

    message_text
}

async fn display_availability_options(bot: &Bot, chat_id: ChatId, username: &Option<String>, existing: &Vec<Availability>, msg_id: Option<MessageId>) -> Option<MessageId> {
    let mut options: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    let mut control_row: Vec<InlineKeyboardButton> = vec![InlineKeyboardButton::callback("ADD", "ADD")];
    let control_options: Vec<InlineKeyboardButton> = ["MODIFY", "DELETE"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    
    if existing.len() > 0 {
        control_row.extend(control_options);
    }
    options.push(control_row);
    options.push(vec![InlineKeyboardButton::callback("DONE", "DONE")]);

    let mut output_text = String::new();
    if existing.is_empty() {
        output_text.push_str("You do not currently have any upcoming available dates indicated.");
    } else {
        output_text.push_str("Here are the upcoming dates for which you have indicated availability:\n");
        for availability in existing {
            let truncated_remarks = if let Some(remarks) = &availability.remarks {
                if remarks.chars().count() > 10 {
                    format!(", {}...", remarks.chars().take(10).collect::<String>())
                } else {
                    format!(", {}", remarks)
                }
            } else {
                "".to_string()
            };

            // Format date as "MMM-DD" (3-letter month)
            let formatted_date = availability.avail.format("%b-%d").to_string();
            
            let state = if availability.is_valid == false && availability.planned == true {
                " (UNAVAIL, PLANNED)"
            } else {
                if availability.planned {
                    " (PLANNED)"
                } else { "" }
            };

            output_text.push_str(&format!(
                "- {}: {}{}{}\n",
                formatted_date, availability.ict_type.as_ref(), state, truncated_remarks
            ));
        }
    }
    
    send_or_edit_msg(bot, chat_id, username, msg_id, output_text, Some(InlineKeyboardMarkup::new(options)), None).await
}

async fn update_availability_edit(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    availability: &Vec<Availability>,
    prefix: &String,
    start: usize,
    show: usize,
    action: &String,
    msg_id: &Option<MessageId>
) -> Result<Option<MessageId>, ()> {
    // Generate the inline keyboard
    let markup = match get_availability_edit_keyboard(availability, prefix, start, show) {
        Ok(kb) => kb,
        Err(_) => {
            send_msg(
                bot.send_message(chat_id, "Error encountered while getting availability."),
                username,
            ).await;
            return Err(());
        }
    };
    
    match msg_id {
        None => {
            // No message to edit, send new message
            Ok(send_msg(
                bot.send_message(chat_id, get_availability_edit_text(availability, start, show, action))
                    .reply_markup(markup),
                username,
            ).await)
        }
        Some(msg_id) => {
            // Message Id to edit
            // Edit both text and reply markup
            Ok(send_or_edit_msg(&bot, chat_id, username, Some(*msg_id), get_availability_edit_text(availability, start, show, action), Some(markup), None).await)
        }
    }
}

async fn handle_re_show_options(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    availability_list: Vec<Availability>,
    prefix: String,
    start: usize,
    show: usize,
    action: String,
    msg_id: Option<MessageId>
) -> HandlerResult {
    match update_availability_edit(&bot, dialogue.chat_id(), username, &availability_list, &prefix, start, show, &action, &msg_id).await {
        Err(_) => dialogue.update(State::ErrorState).await?,
        Ok(new_msg_id) => {
            match new_msg_id {
                None => dialogue.update(State::ErrorState).await?,
                Some(msg_id) => {
                    log::debug!("Transitioning to AvailabilitySelect with MsgId: {:?}, Availability: {:?}, Action: {:?}, Prefix: {:?}, Start: {:?}", msg_id, availability_list, action, prefix, start);
                    dialogue.update(State::AvailabilitySelect { msg_id, availability_list, action, prefix, start }).await?
                }
            }
        }
    };
    Ok(())
}

async fn display_availability_edit_prompt(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    availability_entry: &Availability,
    msg_id: MessageId
) -> Option<MessageId> {
    let edit: Vec<InlineKeyboardButton> = ["TYPE", "REMARKS"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    let options: Vec<InlineKeyboardButton> = ["DELETE", "BACK"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    
    let formatted_date = availability_entry.avail.format("%b-%d").to_string();
    let availability_edit_text = format!(
        "You have indicated availability for: {}\nType: {}\nRemarks: {}\n\n What do you wish to edit?",
        formatted_date,
        availability_entry.ict_type.as_ref(),
        availability_entry.remarks.as_deref().unwrap_or("None")
    );
    
    let keyboard = InlineKeyboardMarkup::new([edit, options]);
    
    // Send or edit message
    send_or_edit_msg(&bot, chat_id, username, Some(msg_id), availability_edit_text, Some(keyboard), None).await
}

async fn display_edit_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let ict_types = [Ict::LIVE, Ict::OTHER]
        .map(|ict_types| InlineKeyboardButton::callback(ict_types.as_ref(), ict_types.as_ref()));
    send_msg(
        bot.send_message(chat_id, "Available for:")
            .reply_markup(InlineKeyboardMarkup::new([ict_types])),
        username
    ).await
}

async fn display_edit_remarks(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    send_msg(
        bot.send_message(chat_id, "Type your remarks:"),
        username,
    ).await
}

async fn update_add_availability(bot: &Bot, chat_id: ChatId, avail_type: &Ict, username: &Option<String>, msg_id: MessageId) -> Option<MessageId> {
    let edit = ["CHANGE TYPE", "CANCEL"]
        .map(|option| InlineKeyboardButton::callback(option, option));
    let message_text = format!(
        "Available for: {}\n\nType the dates for which you want to indicate availability. Use commas(only) to separate dates. (e.g. Jan 2, 28/2, 17/04/24)",
        avail_type.as_ref());
    // Send or edit message
    send_or_edit_msg(&bot, chat_id, username, Some(msg_id), message_text, Some(InlineKeyboardMarkup::new([edit])), None).await
}

async fn display_add_remarks(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let options = ["DONE", "CANCEL"]
        .map(|option| InlineKeyboardButton::callback(option, option));
    
    send_msg(
        bot.send_message(chat_id, "Type your remarks if any (this will be indicated for all the dates you indicated), or /cancel if anything is wrong:")
            .reply_markup(InlineKeyboardMarkup::new([options])),
        username,
    ).await
}

async fn display_delete_confirmation(bot: &Bot, chat_id: ChatId, username: &Option<String>, msg_id: Option<MessageId>, entry: &Availability) -> Option<MessageId> {
    let confirm = ["YES", "NO"]
        .map(|product| InlineKeyboardButton::callback(product, product));
    
    let message_text = format!(
        "You have already been planned on __{}__{}\\.\nConfirm rescind availability?", 
        entry.avail.format("%b\\-%d\\-%Y"),
        if entry.saf100 { " *\\(SAF100 ISSUED\\)*" } else { "" }
    );
    
    send_or_edit_msg(&bot, chat_id, username, msg_id, message_text, Some(InlineKeyboardMarkup::new([confirm])), Some(ParseMode::MarkdownV2)).await
}

async fn delete_availability_entry_and_go_back(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    tele_id: u64,
    availability_entry: Availability,
    start: usize,
    show: usize,
    action: String,
    pool: &PgPool,
    msg_id: Option<MessageId>
) -> HandlerResult {
    
    match controllers::user::get_user_by_tele_id(&pool, tele_id).await {
        Ok(user) => {
            if user.id == availability_entry.user_id {
                match controllers::scheduling::set_user_unavail(&pool, availability_entry.id).await {
                    Ok(details) => {
                        // notify availability
                        notifier::emit::availability_notifications(
                            &bot,
                            format!(
                                "{}{} has specified they are UNAVAIL on {}",
                                details.ops_name,
                                if details.usr_type == UsrType::NS {" \\(NS\\)"} else {""},
                                utils::escape_special_characters(&details.avail.format("%Y-%m-%d").to_string()),
                            ).as_str(),
                            &pool,
                            tele_id as i64
                        ).await;

                        // detect conflicts and notify
                        if details.planned {
                            notifier::emit::conflict_notifications(
                                &bot,
                                format!(
                                    "{}{} has specified they are UNAVAIL on {}\\, but they are PLANNED{}",
                                    details.ops_name,
                                    if details.usr_type == UsrType::NS {" \\(NS\\)"} else {""},
                                    utils::escape_special_characters(&details.avail.format("%Y-%m-%d").to_string()),
                                    if details.saf100 { " SAF100 ISSUED" } else { "" }
                                ).as_str(),
                                &pool,
                            ).await;
                        }

                        let message_text = format!("Deleted entry for: {}", details.avail.format("%b-%d").to_string());
                        send_or_edit_msg(bot, dialogue.chat_id(), username, msg_id, message_text, None, None).await;
                        handle_go_back(bot, dialogue, username, tele_id, start, show, action, pool, None).await?;
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), username).await
                }
            } else {
                dialogue.update(State::ErrorState).await?;
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), username).await
    }
    
    Ok(())
}

async fn handle_go_back(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    tele_id: u64,
    start: usize,
    show: usize,
    action: String,
    pool: &PgPool,
    msg_id: Option<MessageId>
) -> HandlerResult {
    
    // Generate random prefix to make the IDs only applicable to this dialogue instance
    let prefix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect();

    // Retrieve all the pending applications
    match controllers::scheduling::get_upcoming_availability_by_tele_id(pool, tele_id)
        .await {
        Ok(availability_list) => {
            if availability_list.len() == 0 {
                match display_availability_options(bot, dialogue.chat_id(), username, &availability_list, msg_id).await {
                    None => {}
                    Some(msg_id) => {
                        dialogue.update(State::AvailabilityView { msg_id, availability_list }).await?;
                    }
                };
            } else {
                let new_start = if start >= availability_list.len() { max(0, availability_list.len() - show) } else { start };
                handle_re_show_options(bot, dialogue, username, availability_list, prefix, new_start, show, action, msg_id).await?;
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), username).await
    }
    
    Ok(())
}

async fn modify_availability_and_go_back(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    tele_id: u64,
    availability_entry: Availability,
    start: usize,
    show: usize,
    action: String,
    ict_type_edit: Option<Ict>,
    remark_edit: Option<String>,
    pool: &PgPool,
    msg_id: Option<MessageId>
) -> HandlerResult {
    
    match controllers::user::get_user_by_tele_id(&pool, tele_id).await {
        Ok(user) => {
            if user.id == availability_entry.user_id {
                match controllers::scheduling::edit_avail_by_uuid(&pool, availability_entry.id, None, ict_type_edit, remark_edit).await {
                    Ok(updated) => {
                        notifier::emit::availability_notifications(
                            &bot,
                            format!(
                                "{}{} has specified they are AVAIL for {} on {}{}",
                                updated.ops_name,
                                if updated.usr_type == UsrType::NS {" \\(NS\\)"} else {""},
                                updated.ict_type.as_ref(),
                                utils::escape_special_characters(&updated.avail.format("%Y-%m-%d").to_string()),
                                if updated.remarks.is_some() { "\nRemarks: ".to_owned() + utils::escape_special_characters(updated.remarks.as_deref().unwrap_or("none")).as_str() } else { "".to_string() }
                            ).as_str(),
                            &pool,
                            tele_id as i64
                        ).await;
                        
                        let message_text = format!(
                            "Updated entry for: {} to\nType: {}\nRemarks: {}",
                            updated.avail.format("%b-%d").to_string(),
                            updated.ict_type.as_ref(),
                            updated.remarks.as_deref().unwrap_or("None")
                        );

                        send_or_edit_msg(bot, dialogue.chat_id(), username, msg_id, message_text, None, None).await;
                        handle_go_back(bot, dialogue, username, tele_id, start, show, action, pool, None).await?;
                    }
                    Err(_) => {}
                }
            } else {
                dialogue.update(State::ErrorState).await?;
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), username).await
    }
    
    Ok(())
}

async fn register_availability(
    bot: &Bot, 
    dialogue: &MyDialogue, 
    username: &Option<String>,
    tele_id: u64,
    avail_dates: &Vec<NaiveDate>,
    avail_type: &Ict,
    remarks: Option<String>,
    pool: &PgPool,
    msg_id: Option<MessageId>
) {
    // Add the availability to the database for each date
    let mut added: Vec<AvailabilityDetails> = Vec::new();
    for date in avail_dates.iter() {
        match controllers::scheduling::add_user_avail(pool, tele_id, *date, avail_type, remarks.clone(), None).await {
            Ok(details) => {
                added.push(details);
            }
            Err(_) => {}
        }
    }

    if added.is_empty() {
        // Send or edit message
        send_or_edit_msg(&bot, dialogue.chat_id(), username, msg_id, "Error, added no dates.".into(), None, None).await;
    } else {
        let added_dates = added.clone().into_iter().map(|availability| availability.avail).collect();

        notifier::emit::availability_notifications(
            &bot,
            format!(
                "{}{} has specified they are AVAIL for {} on the following dates:\n{}{}",
                added[0].ops_name,
                if added[0].usr_type == UsrType::NS {" \\(NS\\)"} else {""},
                avail_type.as_ref(),
                utils::escape_special_characters(&utils::format_dates_as_markdown(&added_dates)),
                if remarks.is_some() { "\nRemarks: ".to_owned()+utils::escape_special_characters(remarks.as_deref().unwrap_or("\nnone")).as_str() } else { "".to_string() }
            ).as_str(),
            &pool,
            tele_id as i64
        ).await;
        
        // Send or edit message
        send_or_edit_msg(&bot, dialogue.chat_id(), username, msg_id, format!("Added the following dates:\n{}", utils::format_dates_as_markdown(&added_dates)), None, None).await;
    }
}

pub(super) async fn availability(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "availability", "Command", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Retrieve all the pending applications
    match controllers::scheduling::get_upcoming_availability_by_tele_id(&pool, user.id.0)
        .await {
        Ok(availability_list) => {
            match display_availability_options(&bot, dialogue.chat_id(), &user.username, &availability_list, None).await {
                None => {}
                Some(msg_id) => {
                    dialogue.update(State::AvailabilityView { msg_id, availability_list }).await?
                }
            };
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }
    
    Ok(())
}

pub(super) async fn availability_view(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability_list): (MessageId, Vec<Availability>),
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_view", "Callback", q,
        "MessageId" => msg_id,
        "Availability" => availability_list
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => {
            // Generate random prefix to make the IDs only applicable to this dialogue instance
            let prefix: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(5)
                .map(char::from)
                .collect();
            
            let start = 0;
            let show = 8;
            if option == "ADD" {
                let avail_type = Ict::LIVE;
                match update_add_availability(&bot, dialogue.chat_id(), &avail_type, &q.from.username, msg_id).await {
                    None => {}
                    Some(new_msg_id) => {
                        dialogue.update(State::AvailabilityAdd { msg_id: new_msg_id, avail_type }).await?
                    }
                };
            } else if option == "MODIFY" || option == "DELETE" {
                let action = option.clone();
                handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, show, action, Some(msg_id)).await?;
            } else if option == "DONE" {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Returned to start."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            }
        }
    }
    
    Ok(())
}

pub(super) async fn availability_add_callback(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, avail_type): (MessageId, Ict), 
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_callback", "Callback", q,
        "MessageId" => msg_id,
        "Avail Type" => avail_type
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => {
            if option == "CHANGE TYPE" {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                match display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(new_msg_id) => dialogue.update(State::AvailabilityAddChangeType { msg_id, change_type_msg_id: new_msg_id, avail_type }).await?
                }
            } else if option == "CANCEL" {
                log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else {
                dialogue.update(State::ErrorState).await?
            }
        }
    }
    
    Ok(())
}

pub(super) async fn availability_add_change_type(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_type_msg_id, avail_type): (MessageId, MessageId, Ict),
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_change_type", "Callback", q,
        "MessageId" => msg_id,
        "Change Type MessageId" => change_type_msg_id,
        "Avail Type" => avail_type
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => {
            match Ict::from_str(&option) {
                Ok(ict_type_enum) => {
                    if ict_type_enum == Ict::OTHER || ict_type_enum == Ict::LIVE {
                        log_try_delete_msg(&bot, dialogue.chat_id(), change_type_msg_id).await;
                        let avail_type = ict_type_enum;
                        match update_add_availability(&bot, dialogue.chat_id(), &avail_type, &q.from.username, msg_id).await {
                            None => dialogue.update(State::ErrorState).await?,
                            Some(new_msg_id) => dialogue.update(State::AvailabilityAdd { msg_id: new_msg_id, avail_type }).await?
                        };
                    } else {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "Invalid option."),
                            &q.from.username,
                        ).await;
                    }
                }
                _ => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select one, or type /cancel to abort."),
                        &q.from.username,
                    ).await;
                }
            }
        }
    }
    
    Ok(())
}

pub(super) async fn availability_add_message(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, avail_type): (MessageId, Ict),
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_message", "Message", msg,
        "MessageId" => msg_id,
        "Avail Type" => avail_type
    );

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };

    match msg.text().map(ToOwned::to_owned) {
        Some(input_dates_str) => {
            // Parse the dates with ranges
            let (parsed_dates, failed_parsing_dates, duplicate_dates) = utils::parse_dates(&input_dates_str);

            // Check availability for the unique parsed dates
            let availability_results = match controllers::scheduling::check_user_avail_multiple(&pool, user.id.0, parsed_dates.clone()).await {
                Ok(results) => results,
                Err(e) => {
                    log::error!("Error checking availability: {}", e);
                    handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
                    return Ok(());
                }
            };

            // Separate available and unavailable dates
            let mut available_dates = Vec::new();      // Dates the user is available to register
            let mut unavailable_dates = Vec::new();    // Dates the user has already registered

            for (i, avail_opt) in availability_results.into_iter().enumerate() {
                if let Some(avail) = avail_opt {
                    unavailable_dates.push(avail.avail);
                } else {
                    available_dates.push(parsed_dates[i]);
                }
            }

            // Prepare the output for available dates
            let available_str = if available_dates.is_empty() {
                "All the dates have already been registered\\.".to_string()
            } else {
                format!(
                    "*Selected dates:* \n{}\n",
                    utils::escape_special_characters(&utils::format_dates_as_markdown(&available_dates))
                )
            };

            // Prepare separate outputs for different error types
            let mut failed_output_str = String::new();
            let mut unavailable_output_str = String::new();
            let mut duplicate_output_str = String::new();

            if !failed_parsing_dates.is_empty() {
                failed_output_str += &format!(
                    "\n*Failed to parse the following dates:* \n{}\n",
                    utils::escape_special_characters(&utils::format_failed_dates_as_markdown(&failed_parsing_dates))
                );
            }

            if !unavailable_dates.is_empty() {
                let unavailable_dates_str: Vec<String> = unavailable_dates.into_iter().map(|d| d.format("%m/%d/%Y").to_string()).collect();
                unavailable_output_str += &format!(
                    "\n*You have already indicated availability on the following dates\\. Please use the modify function instead:* \n{}\n",
                    utils::escape_special_characters(&utils::format_failed_dates_as_markdown(&unavailable_dates_str))
                );
            }

            if !duplicate_dates.is_empty() {
                duplicate_output_str += &format!(
                    "\n*You have entered duplicate dates:* \n{}\n",
                    utils::escape_special_characters(&utils::format_dates_as_markdown(&duplicate_dates))
                );
            }

            let retry_str = if available_dates.is_empty() && duplicate_dates.is_empty() {
                "No available dates were provided or all provided dates have already been registered\\. Please try again with different dates\\.\n\nType the dates for which you want to indicate availability\\. Use *commas\\(only\\)* to separate dates\\. \\(e\\.g\\. Jan 2\\, 28/2\\, 17/04/24\\)"
            } else {
                ""
            };

            // Combine the messages with clear separation
            let final_output = format!(
                "*Indicated:*\n{}\n{}\n{}\n{}\n{}",
                available_str,
                failed_output_str,
                unavailable_output_str,
                duplicate_output_str,
                retry_str
            );

            // Attempt to edit the original message
            let new_msg_id = send_or_edit_msg(&bot, dialogue.chat_id(), &user.username, Some(msg_id), final_output, None, Some(ParseMode::MarkdownV2)).await;

            match new_msg_id {
                None => {
                    log::error!("Failed to update availability added message in chat ({})", dialogue.chat_id());
                    dialogue.update(State::ErrorState).await?;
                }
                Some(msg_id) => {
                    // Proceed if there are available dates to register
                    if !available_dates.is_empty() {
                        match display_add_remarks(&bot, dialogue.chat_id(), &user.username).await {
                            None => dialogue.update(State::ErrorState).await?,
                            Some(change_msg_id) => dialogue.update(State::AvailabilityAddRemarks { msg_id, change_msg_id, avail_type, avail_dates: available_dates }).await?
                        }
                    }
                }
            }
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter dates, or type /cancel to abort."),
                &user.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn availability_add_remarks(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, avail_type, avail_dates): (MessageId, MessageId, Ict, Vec<NaiveDate>),
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_remarks", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Avail Type" => avail_type,
        "Avail Dates" => avail_dates
    );

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };

    match msg.text().map(ToOwned::to_owned) {
        Some(input_remarks) => {
            // add availability to database with the specified remarks
            log_try_remove_markup(&bot, dialogue.chat_id(), change_msg_id).await;
            register_availability(&bot, &dialogue, &user.username, user.id.0, &avail_dates, &avail_type, Some(input_remarks), &pool, None).await;

            dialogue.update(State::Start).await?;
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter remarks, select DONE if none, or type /cancel to abort."),
                &user.username,
            ).await;
        }
    }
    
    Ok(())
}

pub(super) async fn availability_add_complete(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, avail_type, avail_dates): (MessageId, MessageId, Ict, Vec<NaiveDate>),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_complete", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Avail Type" => avail_type,
        "Avail Dates" => avail_dates
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => {
            if option == "DONE" {
                // add availability to database no remarks
                log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                register_availability(&bot, &dialogue, &q.from.username, q.from.id.0, &avail_dates, &avail_type, None, &pool, Some(change_msg_id)).await;
                
                dialogue.update(State::Start).await?
            } else if option == "CANCEL" {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else {
                dialogue.update(State::ErrorState).await?
            }
        }
    }

    Ok(())
}

pub(super) async fn availability_select(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability_list, action, prefix, start): (MessageId, Vec<Availability>, String, String, usize),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_modify", "Callback", q,
        "MessageId" => msg_id,
        "Availability" => availability_list,
        "Action" => action,
        "Prefix" => prefix,
        "Start" => start
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, 8, action, Some(msg_id)).await?;
        }
        Some(option) => {
            if option == "PREV" {
                handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, max(0, start as i64 -8) as usize, 8, action, Some(msg_id)).await?;
            } else if option == "NEXT" {
                let entries_len = availability_list.len();
                handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, if start+8 < entries_len { start+8 } else { start }, 8, action, Some(msg_id)).await?;
            } else if option == "CANCEL" {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else if option == "DONE" {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Done."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else {
                match option.strip_prefix(&prefix) {
                    None => handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, 8, action, Some(msg_id)).await?,
                    Some(id) => {
                        match Uuid::try_parse(&id) {
                            Ok(parsed_id) => {
                                match controllers::scheduling::get_availability_by_uuid(&pool, parsed_id).await {
                                    Ok(availability_entry) => {
                                        if action == "MODIFY" {
                                            match display_availability_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &availability_entry, msg_id).await {
                                                None => dialogue.update(State::ErrorState).await?,
                                                Some(msg_id) => {
                                                    log::debug!("Transitioning to AvailabilityModify with MessageId: {:?}, Availability: {:?}, Action: {:?}, Start: {:?}", msg_id, availability_entry, action, start);
                                                    dialogue.update(State::AvailabilityModify { msg_id, availability_entry, action, start }).await?;
                                                }
                                            };
                                        } else if action == "DELETE" {
                                            if availability_entry.planned {
                                                match display_delete_confirmation(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), &availability_entry).await {
                                                    None => dialogue.update(State::ErrorState).await?,
                                                    Some(new_msg_id) => dialogue.update(State::AvailabilityDeleteConfirm { msg_id: new_msg_id, availability_entry, action, start }).await?
                                                }
                                            } else {
                                                delete_availability_entry_and_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, availability_entry, start, 8, action, &pool, Some(msg_id)).await?;
                                            }
                                        } else {
                                            dialogue.update(State::ErrorState).await?;
                                        }
                                    }
                                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                                }
                            }
                            Err(_) => handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, 8, action, Some(msg_id)).await?,
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn availability_modify(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability_entry, action, start): (MessageId, Availability, String, usize),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_modify", "Callback", q,
        "MessageId" => msg_id,
        "Availability" => availability_entry,
        "Action" => action,
        "Start" => start
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => {
            if option == "TYPE" {
                match display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(change_msg_id) => {
                        log::debug!("Transitioning to AvailabilityModifyType with Availability: {:?}, Action: {:?}, Start: {:?}", availability_entry, action, start);
                        dialogue.update(State::AvailabilityModifyType { msg_id, change_msg_id, availability_entry, action, start }).await?
                    }
                }
            } else if option == "REMARKS" {
                match display_edit_remarks(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(change_msg_id) => {
                        log::debug!("Transitioning to AvailabilityModifyRemarks with Availability: {:?}, Action: {:?}, Start: {:?}", availability_entry, action, start);
                        dialogue.update(State::AvailabilityModifyRemarks { msg_id, change_msg_id, availability_entry, action, start }).await?
                    }
                }
                
            } else if option == "DELETE" {
                if availability_entry.planned {
                    match display_delete_confirmation(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), &availability_entry).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => dialogue.update(State::AvailabilityDeleteConfirm { msg_id: new_msg_id, availability_entry, action, start }).await?
                    }
                } else {
                    delete_availability_entry_and_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, availability_entry, start, 8, action, &pool, Some(msg_id)).await?;
                }
            } else if option == "BACK" {
                handle_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, start, 8, action, &pool, Some(msg_id)).await?;
            }
        }
    }

    Ok(())
}

pub(super) async fn availability_modify_remarks(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, availability_entry, action, start): (MessageId, MessageId, Availability, String, usize),
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_modify_remarks", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Availability" => availability_entry,
        "Action" => action,
        "Start" => start
    );
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };
    
    match msg.text().map(ToOwned::to_owned) {
        Some(input_remarks) => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            modify_availability_and_go_back(&bot, &dialogue, &user.username, user.id.0, availability_entry, start, 8, action, None, Some(input_remarks), &pool, None).await?;
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter remarks, or type /cancel to abort."),
                &user.username,
            ).await;
        }
    }
    
    Ok(())
}

pub(super) async fn availability_modify_type(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, availability_entry, action, start): (MessageId, MessageId, Availability, String, usize),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_modify_type", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Availability" => availability_entry,
        "Action" => action,
        "Start" => start
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => {
            match Ict::from_str(&option) {
                Ok(ict_type_enum) => {
                    if ict_type_enum == Ict::OTHER || ict_type_enum == Ict::LIVE {
                        log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                        send_msg(
                            bot.send_message(dialogue.chat_id(), format!("Selected type: `{}`", ict_type_enum.as_ref())).parse_mode(ParseMode::MarkdownV2),
                            &q.from.username,
                        ).await;
                        modify_availability_and_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, availability_entry, start, 8, action, Some(ict_type_enum), None, &pool, Some(msg_id)).await?;
                    } else {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "Invalid option."),
                            &q.from.username,
                        ).await;
                    }
                }
                _ => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select one, or type /cancel to abort."),
                        &q.from.username,
                    ).await;
                }
            }
        }
    }
    
    Ok(())
}

pub(super) async fn availability_delete_confirm(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, availability_entry, action, start): (MessageId, Availability, String, usize),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_delete_confirm", "Callback", q,
        "MessageId" => msg_id,
        "Availability" => availability_entry,
        "Action" => action,
        "Start" => start
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => {
            if option == "YES" {
                delete_availability_entry_and_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, availability_entry, start, 8, action, &pool, Some(msg_id)).await?;
            } else if option == "NO" {
                match display_availability_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &availability_entry, msg_id).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(msg_id) => {
                        log::debug!("Transitioning to AvailabilityModify with MessageId: {:?}, Availability: {:?}, Action: {:?}, Start: {:?}", msg_id, availability_entry, action, start);
                        dialogue.update(State::AvailabilityModify { msg_id, availability_entry, action, start }).await?;
                    }
                };
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
