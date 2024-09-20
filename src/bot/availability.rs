use crate::bot::state::State;
use crate::bot::{handle_error, send_msg, HandlerResult, MyDialogue};
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
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId};

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
                if remarks.len() > 8 {
                    format!(", {}...", &remarks[0..8])
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

async fn display_availability_options(bot: &Bot, chat_id: ChatId, username: &Option<String>, existing: &Vec<Availability>) {
    let mut options: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    options.push(vec![InlineKeyboardButton::callback("ADD", "ADD")]);
    
    let control_options: Vec<InlineKeyboardButton> = ["ADD", "MODIFY", "DELETE"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    
    if existing.len() > 0 {
        options.push(control_options);
    }
    options.push(vec![InlineKeyboardButton::callback("DONE", "DONE")]);

    let mut output_text = String::new();
    if existing.is_empty() {
        output_text.push_str("You do not currently have any upcoming available dates indicated.");
    } else {
        output_text.push_str("Here are the upcoming dates for which you have indicated availability:\n");
        for availability in existing {
            let truncated_remarks = if let Some(remarks) = &availability.remarks {
                if remarks.len() > 10 {
                    format!(", {}...", &remarks[0..10])
                } else {
                    format!(", {}", &remarks)
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

    send_msg(
        bot.send_message(chat_id, output_text)
            .reply_markup(InlineKeyboardMarkup::new(options)),
        username
    ).await;
}

async fn display_availability_edit(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    availability: &Vec<Availability>,
    prefix: &String,
    start: usize,
    show: usize,
    action: &String
) -> Result<MessageId, ()> {
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

    // Generate the message text
    let message_text = get_availability_edit_text(availability, start, show, action);

    // Send the message and capture the MessageId
    match send_msg(
        bot.send_message(chat_id, message_text)
            .reply_markup(markup),
        username
    ).await {
        Some(msg_id) => Ok(msg_id),
        None => Err(())
    }
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
    msg_id: &MessageId
) -> Result<(), ()> {
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

    // Edit both text and reply markup in one call
    match bot.edit_message_text(chat_id, *msg_id, get_availability_edit_text(availability, start, show, action))
        .reply_markup(markup)
        .await {
        Ok(_) => {}
        Err(e) => {
            log::error!(
                "Error editing msg in response to user: {}, error: {}",
                username.as_deref().unwrap_or("none"),
                e
            );
        }
    };

    Ok(())
}

async fn handle_show_options(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    availability_list: Vec<Availability>,
    prefix: String,
    start: usize,
    show: usize,
    action: String,
) -> HandlerResult {
    match display_availability_edit(&bot, dialogue.chat_id(), username, &availability_list, &prefix, start, show, &action)
        .await {
        Err(_) => dialogue.update(State::ErrorState).await?,
        Ok(msg_id) => {
            log::debug!("Transitioning to AvailabilitySelect with MsgId: {:?}, Availability: {:?}, Action: {:?}, Prefix: {:?}, Start: {:?}", msg_id, availability_list, action, prefix, start);
            dialogue.update(State::AvailabilitySelect { msg_id, availability_list, action, prefix, start }).await?;
        }
    }
    Ok(())
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
    msg_id: MessageId
) -> HandlerResult {
    match update_availability_edit(&bot, dialogue.chat_id(), username, &availability_list, &prefix, start, show, &action, &msg_id).await {
        Err(_) => dialogue.update(State::ErrorState).await?,
        Ok(_) => {
            log::debug!("Transitioning to AvailabilitySelect with MsgId: {:?}, Availability: {:?}, Action: {:?}, Prefix: {:?}, Start: {:?}", msg_id, availability_list, action, prefix, start);
            dialogue.update(State::AvailabilitySelect { msg_id, availability_list, action, prefix, start }).await?;
        }
    };
    Ok(())
}

async fn display_availability_edit_prompt(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    availability_entry: &Availability
) {
    let edit: Vec<InlineKeyboardButton> = ["TYPE", "REMARKS"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    let options: Vec<InlineKeyboardButton> = ["DELETE", "BACK"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    
    let formatted_date = availability_entry.avail.format("%b-%d").to_string();
    send_msg(
        bot.send_message(
            chat_id,
            format!(
                "You have indicated availability for: {}\nType: {}\nRemarks: {}\n\n What do you wish to edit?", 
                formatted_date,
                availability_entry.ict_type.as_ref(),
                availability_entry.remarks.as_deref().unwrap_or("None")
            )
        )
            .reply_markup(InlineKeyboardMarkup::new([edit, options])),
        username
    ).await;
}

async fn display_edit_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let ict_types = [Ict::LIVE, Ict::OTHER]
        .map(|ict_types| InlineKeyboardButton::callback(ict_types.as_ref(), ict_types.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Available for:")
            .reply_markup(InlineKeyboardMarkup::new([ict_types])),
        username
    ).await;
}

async fn display_edit_remarks(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    send_msg(
        bot.send_message(chat_id, "Type your remarks:"),
        username,
    ).await;
}

async fn display_add_availability(bot: &Bot, chat_id: ChatId, username: &Option<String>, avail_type: &Ict) {
    let edit = ["CHANGE TYPE", "CANCEL"]
        .map(|option| InlineKeyboardButton::callback(option, option));
    
    send_msg(
        bot.send_message(chat_id, format!(
            "Available for: {}\n\nType the dates for which you want to indicate availability. Use commas(only) to separate dates. (e.g. Jan 2, 28/2, 17/04/24)",
            avail_type.as_ref())
        ).reply_markup(InlineKeyboardMarkup::new([edit])),
        username,
    ).await;
}

async fn display_add_remarks(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let options = ["DONE", "CANCEL"]
        .map(|option| InlineKeyboardButton::callback(option, option));
    
    send_msg(
        bot.send_message(chat_id, "Type your remarks if any (this will be indicated for all the dates you indicated), or /cancel if anything is wrong:")
            .reply_markup(InlineKeyboardMarkup::new([options])),
        username,
    ).await;
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
    pool: &PgPool
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
                                if details.usr_type == UsrType::NS {" (NS)"} else {""},
                                details.avail.format("%Y-%m-%d")
                            ).as_str(),
                            &pool,
                        ).await;

                        // detect conflicts and notify
                        if details.planned {
                            notifier::emit::conflict_notifications(
                                &bot,
                                format!(
                                    "{}{} has specified they are UNAVAIL on {}, but they are PLANNED{}",
                                    details.ops_name,
                                    if details.usr_type == UsrType::NS {" (NS)"} else {""},
                                    details.avail.format("%Y-%m-%d"),
                                    if details.saf100 { " SAF100 ISSUED" } else { "" }
                                ).as_str(),
                                &pool,
                            ).await;
                        }

                        send_msg(
                            bot.send_message(dialogue.chat_id(), format!("Deleted entry for: {}", details.avail.format("%b-%d").to_string())),
                            username,
                        ).await;
                        handle_go_back(bot, dialogue, username, tele_id, start, show, action, pool).await?;
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
    pool: &PgPool
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
                display_availability_options(bot, dialogue.chat_id(), username, &availability_list).await;
                dialogue.update(State::AvailabilityView { availability_list }).await?;
            } else {
                let new_start = if start >= availability_list.len() { max(0, availability_list.len() - show) } else { start };
                handle_show_options(bot, dialogue, username, availability_list, prefix, new_start, show, action).await?;
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
    pool: &PgPool
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
                                if updated.usr_type == UsrType::NS {" (NS)"} else {""},
                                updated.ict_type.as_ref(),
                                updated.avail.format("%Y-%m-%d"),
                                if updated.remarks.is_some() { "\nRemarks: ".to_owned()+updated.remarks.as_deref().unwrap_or("none") } else { "".to_string() }
                            ).as_str(),
                            &pool,
                        ).await;

                        send_msg(
                            bot.send_message(dialogue.chat_id(), format!(
                                "Updated entry for: {} to\nType: {}\nRemarks: {}", 
                                updated.avail.format("%b-%d").to_string(),
                                updated.ict_type.as_ref(),
                                updated.remarks.as_deref().unwrap_or("None")
                            )),
                            username,
                        ).await;
                        handle_go_back(bot, dialogue, username, tele_id, start, show, action, pool).await?;
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
    pool: &PgPool
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
        send_msg(
            bot.send_message(dialogue.chat_id(), "Error, added no dates."),
            username,
        ).await;
    } else {
        let added_dates = added.clone().into_iter().map(|availability| availability.avail).collect();

        notifier::emit::availability_notifications(
            &bot,
            format!(
                "{}{} has specified they are AVAIL for {} on the following dates:\n{}{}",
                added[0].ops_name,
                if added[0].usr_type == UsrType::NS {" (NS)"} else {""},
                avail_type.as_ref(),
                utils::format_dates_as_markdown(&added_dates),
                if remarks.is_some() { "\nRemarks: ".to_owned()+remarks.as_deref().unwrap_or("\nnone") } else { "".to_string() }
            ).as_str(),
            &pool,
        ).await;

        send_msg(
            bot.send_message(dialogue.chat_id(), format!("Added the following dates:\n{}", utils::format_dates_as_markdown(&added_dates))),
            username,
        ).await;
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
            display_availability_options(&bot, dialogue.chat_id(), &user.username, &availability_list).await;
            dialogue.update(State::AvailabilityView { availability_list }).await?
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }
    
    Ok(())
}

pub(super) async fn availability_view(
    bot: Bot,
    dialogue: MyDialogue,
    availability_list: Vec<Availability>,
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_view", "Callback", q,
        "Availability" => availability_list
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            display_availability_options(&bot, dialogue.chat_id(), &q.from.username, &availability_list).await;
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
                display_add_availability(&bot, dialogue.chat_id(), &q.from.username, &avail_type).await;
                dialogue.update(State::AvailabilityAdd { avail_type }).await?
            } else if option == "MODIFY" || option == "DELETE" {
                let action = option.clone();
                handle_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, show, action).await?;
            } else if option == "DONE" {
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
    avail_type: Ict, 
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_callback", "Callback", q,
        "Avail Type" => avail_type
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            display_add_availability(&bot, dialogue.chat_id(), &q.from.username, &avail_type).await;
        }
        Some(option) => {
            if option == "CHANGE TYPE" {
                display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
                dialogue.update(State::AvailabilityAddChangeType { avail_type }).await?
            } else if option == "CANCEL" {
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
    avail_type: Ict,
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_change_type", "Callback", q,
        "Avail Type" => avail_type
    );


    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
        }
        Some(option) => {
            match Ict::from_str(&option) {
                Ok(ict_type_enum) => {
                    if ict_type_enum == Ict::OTHER || ict_type_enum == Ict::LIVE {
                        let avail_type = ict_type_enum;
                        display_add_availability(&bot, dialogue.chat_id(), &q.from.username, &avail_type).await;
                        dialogue.update(State::AvailabilityAdd { avail_type }).await?
                    } else {
                        display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
                    }
                }
                _ => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select one, or type /cancel to abort."),
                        &q.from.username,
                    ).await;
                    display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
                }
            }
        }
    }
    
    Ok(())
}

pub(super) async fn availability_add_message(
    bot: Bot,
    dialogue: MyDialogue,
    avail_type: Ict,
    msg: Message,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_message", "Message", msg,
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
        Some(input_dates) => {
            // parse date here into Vec<NaiveDate>
            let parsed_dates = utils::parse_dates(input_dates.as_str());
            // display to the user the dates they have entered
            let output_str = utils::format_dates_as_markdown(&parsed_dates);
            send_msg(
                bot.send_message(dialogue.chat_id(), format!("Indicated:\n{}", output_str)),
                &user.username,
            ).await;
            display_add_remarks(&bot, dialogue.chat_id(), &user.username).await;
            dialogue.update(State::AvailabilityAddRemarks { avail_type, avail_dates: parsed_dates }).await?
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter dates, or type /cancel to abort."),
                &user.username,
            ).await;
            display_add_availability(&bot, dialogue.chat_id(), &user.username, &avail_type).await;
        }
    }
    
    Ok(())
}

pub(super) async fn availability_add_remarks(
    bot: Bot,
    dialogue: MyDialogue,
    (avail_type, avail_dates): (Ict, Vec<NaiveDate>),
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_remarks", "Message", msg,
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
            register_availability(&bot, &dialogue, &user.username, user.id.0, &avail_dates, &avail_type, Some(input_remarks), &pool).await;

            dialogue.update(State::Start).await?;
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter remarks, select DONE if none, or type /cancel to abort."),
                &user.username,
            ).await;
            display_add_remarks(&bot, dialogue.chat_id(), &user.username).await;
        }
    }
    
    Ok(())
}

pub(super) async fn availability_add_complete(
    bot: Bot,
    dialogue: MyDialogue,
    (avail_type, avail_dates): (Ict, Vec<NaiveDate>),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_add_complete", "Callback", q,
        "Avail Type" => avail_type,
        "Avail Dates" => avail_dates
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            display_add_availability(&bot, dialogue.chat_id(), &q.from.username, &avail_type).await;
        }
        Some(option) => {
            if option == "DONE" {
                // add availability to database with the specified remarks
                register_availability(&bot, &dialogue, &q.from.username, q.from.id.0, &avail_dates, &avail_type, None, &pool).await;
                
                dialogue.update(State::Start).await?
            } else if option == "CANCEL" {
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

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, 8, action, msg_id).await?;
        }
        Some(option) => {
            if option == "PREV" {
                handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, max(0, start-8), 8, action, msg_id).await?;
            } else if option == "NEXT" {
                let entries_len = availability_list.len();
                handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, if start+8 < entries_len { start+8 } else { start }, 8, action, msg_id).await?;
            } else if option == "CANCEL" {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else if option == "DONE" {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Done."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else {
                match option.strip_prefix(&prefix) {
                    None => handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, 8, action, msg_id).await?,
                    Some(id) => {
                        match Uuid::try_parse(&id) {
                            Ok(parsed_id) => {
                                match controllers::scheduling::get_availability_by_uuid(&pool, parsed_id).await {
                                    Ok(availability_entry) => {
                                        if action == "MODIFY" {
                                            display_availability_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &availability_entry).await;
                                            log::debug!("Transitioning to AvailabilityModify with Availability: {:?}, AvailabilityList: {:?}, Action: {:?}, Prefix: {:?}, Start: {:?}", availability_entry, availability_list, action, prefix, start);
                                            dialogue.update(State::AvailabilityModify { availability_entry, availability_list, action, prefix, start }).await?;
                                        } else if action == "DELETE" {
                                            delete_availability_entry_and_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, availability_entry, start, 8, action, &pool).await?;
                                        } else {
                                            dialogue.update(State::ErrorState).await?;
                                        }
                                    }
                                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                                }
                            }
                            Err(_) => handle_re_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, 8, action, msg_id).await?,
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
    (availability_entry, availability_list, action, prefix, start): (Availability, Vec<Availability>, String, String, usize),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_modify", "Callback", q,
        "Availability" => availability_entry,
        "Action" => action,
        "Prefix" => prefix,
        "Start" => start
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            display_availability_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &availability_entry).await;
        }
        Some(option) => {
            if option == "TYPE" {
                display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
                log::debug!("Transitioning to AvailabilityModifyType with Availability: {:?}, Action: {:?}, Start: {:?}", availability_entry, action, start);
                dialogue.update(State::AvailabilityModifyType { availability_entry, action, start }).await?;
            } else if option == "REMARKS" {
                display_edit_remarks(&bot, dialogue.chat_id(), &q.from.username).await;
                log::debug!("Transitioning to AvailabilityModifyRemarks with Availability: {:?}, Action: {:?}, Start: {:?}", availability_entry, action, start);
                dialogue.update(State::AvailabilityModifyRemarks { availability_entry, action, start }).await?;
            } else if option == "DELETE" {
                delete_availability_entry_and_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, availability_entry, start, 8, action, &pool).await?;
            } else if option == "BACK" {
                handle_show_options(&bot, &dialogue, &q.from.username, availability_list, prefix, start, 8, action).await?;
            }
        }
    }

    Ok(())
}

pub(super) async fn availability_modify_remarks(
    bot: Bot,
    dialogue: MyDialogue,
    (availability_entry, action, start): (Availability, String, usize),
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_modify_remarks", "Message", msg,
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
            modify_availability_and_go_back(&bot, &dialogue, &user.username, user.id.0, availability_entry, start, 8, action, None, Some(input_remarks), &pool).await?;
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter remarks, or type /cancel to abort."),
                &user.username,
            ).await;
            display_edit_remarks(&bot, dialogue.chat_id(), &user.username).await;
        }
    }
    
    Ok(())
}

pub(super) async fn availability_modify_type(
    bot: Bot,
    dialogue: MyDialogue,
    (availability_entry, action, start): (Availability, String, usize),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "availability_modify_type", "Callback", q,
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
            display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
        }
        Some(option) => {
            match Ict::from_str(&option) {
                Ok(ict_type_enum) => {
                    if ict_type_enum == Ict::OTHER || ict_type_enum == Ict::LIVE {
                        modify_availability_and_go_back(&bot, &dialogue, &q.from.username, q.from.id.0, availability_entry, start, 8, action, Some(ict_type_enum), None, &pool).await?;
                    } else {
                        display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
                    }
                }
                _ => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select one, or type /cancel to abort."),
                        &q.from.username,
                    ).await;
                    display_edit_types(&bot, dialogue.chat_id(), &q.from.username).await;
                }
            }
        }
    }
    
    Ok(())
}
