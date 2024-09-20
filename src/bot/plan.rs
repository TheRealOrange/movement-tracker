use super::{handle_error, send_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::types::{AvailabilityDetails, RoleType, Usr, UsrType};
use crate::{controllers, log_endpoint_hit, notifier, utils};
use chrono::NaiveDate;
use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::PgPool;
use std::cmp::{max, min};
use std::str::FromStr;
use strum::IntoEnumIterator;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};
use uuid::Uuid;

// Generates the inline keyboard for user availability view
fn get_user_availability_keyboard(
    prefix: &String,
    availability_list: &Vec<AvailabilityDetails>,
    start: usize,
    show: usize
) -> Result<InlineKeyboardMarkup, ()> {
    let slice_end = min(start + show, availability_list.len());
    let shown_entries = match availability_list.get(start..slice_end) {
        Some(entries) => entries,
        None => {
            log::error!("Cannot get availability entries slice");
            return Err(());
        }
    };

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries
        .iter()
        .map(|entry| {
            let option_str = if entry.planned {
                if entry.is_valid { "UNPLAN" } else { "UNPLAN (UNAVAIL)" }
            } else {
                "PLAN"
            };
            let truncated_remarks = if let Some(remarks) = &entry.remarks {
                if remarks.len() > 8 {
                    format!(", {}...", &remarks[0..8])
                } else {
                    format!(", {}", &remarks)
                }
            } else {
                "".to_string()
            };
            // Format date as "MMM-DD"
            let formatted = format!(
                "{} {}: {}{}",
                option_str,
                entry.avail.format("%b-%d"),
                entry.ict_type.as_ref(),
                truncated_remarks
            );
            vec![InlineKeyboardButton::callback(
                formatted,
                format!("{}{}", prefix, entry.id)
            )]
        })
        .collect();

    // Add "PREV", "NEXT", and "DONE" buttons
    let mut pagination = Vec::new();
    if start > 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", "PREV"));
    }
    if slice_end < availability_list.len() {
        pagination.push(InlineKeyboardButton::callback("NEXT", "NEXT"));
    }
    pagination.push(InlineKeyboardButton::callback("DONE", "DONE"));

    entries.push(pagination);

    Ok(InlineKeyboardMarkup::new(entries))
}

// Generates the inline keyboard for date availability view
fn get_date_availability_keyboard(
    prefix: &String,
    availability_list: &Vec<AvailabilityDetails>,
    role_type: &RoleType,
    start: usize,
    show: usize
) -> Result<InlineKeyboardMarkup, ()> {
    let slice_end = min(start + show, availability_list.len());
    let shown_entries = match availability_list.get(start..slice_end) {
        Some(entries) => entries,
        None => {
            log::error!("Cannot get availability entries slice");
            return Err(());
        }
    };

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries
        .iter()
        .map(|entry| {
            let option_str = if entry.planned {
                if entry.is_valid { "UNPLAN" } else { "UNPLAN *(UNAVAIL)*" }
            } else {
                "PLAN"
            };
            let truncated_remarks = if let Some(remarks) = &entry.remarks {
                if remarks.len() > 8 {
                    format!(", {}...", &remarks[0..8])
                } else {
                    format!(", {}", &remarks)
                }
            } else {
                "".to_string()
            };

            let formatted = format!(
                "{} {}: {}{}",
                option_str,
                entry.ops_name,
                entry.ict_type.as_ref(),
                truncated_remarks
            );
            vec![InlineKeyboardButton::callback(
                formatted,
                format!("{}{}", prefix, entry.id)
            )]
        })
        .collect();

    // Add role change buttons if applicable
    let change_view_roles: Vec<InlineKeyboardButton> = RoleType::iter()
        .filter_map(|role| {
            if *role_type != role {
                Some(InlineKeyboardButton::callback(
                    format!("VIEW {}", role.as_ref()),
                    role.as_ref()
                ))
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
    if slice_end < availability_list.len() {
        pagination.push(InlineKeyboardButton::callback("NEXT", "NEXT"));
    }
    pagination.push(InlineKeyboardButton::callback("DONE", "DONE"));

    entries.push(change_view_roles);
    entries.push(pagination);

    Ok(InlineKeyboardMarkup::new(entries))
}

// Generates the message text for user availability view
fn get_user_availability_text(
    user_details: &Usr,
    availability_list: &Vec<AvailabilityDetails>
) -> String {
    let mut message = format!(
        "Availability for {}:\n",
        utils::escape_special_characters(&user_details.ops_name)
    );

    if availability_list.is_empty() {
        message.push_str("No upcoming availability\\.\n");
    } else {
        for availability in availability_list {
            let date_str = availability.avail.format("%b %d, %Y").to_string();
            let ict_type_str = availability.ict_type.as_ref();
            let planned_str = if availability.planned { " \\(PLANNED\\)" } else { "" };
            let avail_str = if availability.is_valid { "" } else { " *\\(UNAVAIL\\)*" };
            let usrtype_str = if availability.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
            let saf100_str = if availability.saf100 { " SAF100 ISSUED" }
            else if availability.planned && availability.usr_type == UsrType::NS { " *PENDING SAF100*" }
            else { "" };

            // Truncate remarks to a max of 15 characters
            let remarks_str = if let Some(remarks) = &availability.remarks {
                if remarks.len() > 15 {
                    format!("{}: {}...", saf100_str, utils::escape_special_characters(&remarks[0..15]))
                } else {
                    format!("{}: {}", saf100_str, utils::escape_special_characters(&remarks))
                }
            } else {
                saf100_str.to_string()
            };

            message.push_str(&format!(
                "\\- {} __{}__ {}{}{}{}\n",
                date_str,
                ict_type_str,
                planned_str,
                avail_str,
                usrtype_str,
                remarks_str
            ));
        }
    }

    message
}

// Generates the message text for date availability view
fn get_date_availability_text(
    selected_date: &NaiveDate,
    availability_list: &Vec<AvailabilityDetails>
) -> String {
    let date_str = selected_date.format("%b %d, %Y").to_string();
    let mut message = format!(
        "Users available on {}:\n",
        utils::escape_special_characters(&date_str)
    );

    if availability_list.is_empty() {
        message.push_str("No users available on this date\\.\n");
    } else {
        for availability in availability_list {
            let ict_type_str = availability.ict_type.as_ref();
            let planned_str = if availability.planned { " \\(PLANNED\\)" } else { "" };
            let avail_str = if availability.is_valid { "" } else { " *\\(UNAVAIL\\)*" };
            let usrtype_str = if availability.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
            let saf100_str = if availability.saf100 { " SAF100 ISSUED" }
            else if availability.planned && availability.usr_type == UsrType::NS { " *PENDING SAF100*" }
            else { "" };

            // Truncate remarks to a max of 15 characters
            let remarks_str = if let Some(remarks) = &availability.remarks {
                if remarks.len() > 15 {
                    format!("{}: {}...", saf100_str, utils::escape_special_characters(&remarks[0..15]))
                } else {
                    format!("{}: {}", saf100_str, utils::escape_special_characters(&remarks))
                }
            } else {
                saf100_str.to_string()
            };

            message.push_str(&format!(
                "\\- {} __{}__ {}{}{}{}\n",
                availability.ops_name,
                ict_type_str,
                planned_str,
                avail_str,
                usrtype_str,
                remarks_str
            ));
        }
    }

    message
}

// Displays user availability with pagination using message editing
async fn display_user_availability(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    user_details: &Usr,
    availability_list: &Vec<AvailabilityDetails>,
    prefix: &String,
    start: usize,
    show: usize,
    msg_id: Option<MessageId>, // Optionally provide MessageId to edit
) -> Result<MessageId, ()> {
    // Generate the inline keyboard
    let markup = match get_user_availability_keyboard(prefix, availability_list, start, show) {
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
    let message_text = get_user_availability_text(user_details, availability_list);

    // Send or edit the message
    match msg_id {
        Some(id) => {
            // Edit the existing message
            match bot.edit_message_text(chat_id, id, message_text)
                .parse_mode(ParseMode::MarkdownV2)
                .reply_markup(markup)
                .await
            {
                Ok(_) => Ok(id),
                Err(e) => {
                    log::error!("Failed to edit message: {}", e);
                    Err(())
                }
            }
        }
        None => {
            // Send a new message
            match send_msg(
                bot.send_message(chat_id, message_text)
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(markup),
                username
            ).await {
                Some(sent_msg) => Ok(sent_msg),
                None => Err(())
            }
        }
    }
}

// Displays date availability with pagination using message editing
async fn display_date_availability(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    selected_date: &NaiveDate,
    availability_list: &Vec<AvailabilityDetails>,
    role_type: &RoleType,
    prefix: &String,
    start: usize,
    show: usize,
    msg_id: Option<MessageId>, // Optionally provide MessageId to edit
) -> Result<MessageId, ()> {
    // Generate the inline keyboard
    let markup = match get_date_availability_keyboard(prefix, availability_list, role_type, start, show) {
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
    let message_text = get_date_availability_text(selected_date, availability_list);

    // Send or edit the message
    match msg_id {
        Some(id) => {
            // Edit the existing message
            match bot.edit_message_text(chat_id, id, message_text)
                .parse_mode(ParseMode::MarkdownV2)
                .reply_markup(markup)
                .await
            {
                Ok(_) => Ok(id),
                Err(e) => {
                    log::error!("Failed to edit message: {}", e);
                    Err(())
                }
            }
        }
        None => {
            // Send a new message
            match send_msg(
                bot.send_message(chat_id, message_text)
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(markup),
                username
            ).await {
                Some(sent_msg) => Ok(sent_msg),
                None => Err(())
            }
        }
    }
}

async fn handle_show_avail_by_user(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    user_details: Usr,
    availability_list: Vec<AvailabilityDetails>,
    role_type: RoleType,
    prefix: String,
    start: usize,
    show: usize,
) -> HandlerResult {
    // Viewing availability by user
    match display_user_availability(
        bot,
        dialogue.chat_id(),
        username,
        &user_details,
        &availability_list,
        &prefix,
        start,
        show,
        None,
    ).await {
        Ok(msg_id) => {
            log::debug!("Transition to PlanView (viewing by user) with MsgId: {:?}, User: {:?}, Start: {:?}", msg_id, user_details, start);
            dialogue.update(State::PlanView {
                msg_id,
                user_details: Some(user_details),
                selected_date: None,
                availability_list,
                role_type,
                prefix,
                start
            }).await?;
        }
        Err(_) => dialogue.update(State::ErrorState).await?,
    };

    Ok(())
}

async fn handle_show_avail_by_date(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    selected_date: NaiveDate,
    availability_list: Vec<AvailabilityDetails>,
    role_type: RoleType,
    prefix: String,
    start: usize,
    show: usize,
) -> HandlerResult {
    // Viewing availability by date
    match display_date_availability(
        bot,
        dialogue.chat_id(),
        username,
        &selected_date,
        &availability_list,
        &role_type,
        &prefix,
        start,
        show,
        None
    ).await {
        Ok(msg_id) => {
            log::debug!("Transition to PlanView (viewing by date) with MsgId: {:?}, Date: {:?}, Start: {:?}", msg_id, selected_date, start);
            dialogue.update(State::PlanView {
                msg_id,
                user_details: None,
                selected_date: Some(selected_date),
                availability_list,
                role_type,
                prefix,
                start
            }).await?;
        }
        Err(_) => dialogue.update(State::ErrorState).await?,
    };

    Ok(())
}

// Handles updating the availability view (both user and date) based on the current state
async fn handle_re_show_options(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    user_details: Option<Usr>,
    selected_date: Option<NaiveDate>,
    availability_list: Vec<AvailabilityDetails>,
    role_type: RoleType,
    prefix: String,
    start: usize,
    show: usize,
    msg_id: MessageId, // Existing MessageId to edit
) -> HandlerResult {
    match user_details {
        Some(user_details) => {
            // Viewing availability by user
            match display_user_availability(
                bot,
                dialogue.chat_id(),
                username,
                &user_details,
                &availability_list,
                &prefix,
                start,
                show,
                Some(msg_id),
            ).await {
                Ok(_) => {
                    log::debug!("Transition to PlanView (viewing by user) with MsgId: {:?}, User: {:?}, Start: {:?}", msg_id, user_details, start);
                    dialogue.update(State::PlanView {
                        msg_id,
                        user_details: Some(user_details),
                        selected_date: None,
                        availability_list,
                        role_type,
                        prefix,
                        start
                    }).await?;
                }
                Err(_) => dialogue.update(State::ErrorState).await?,
            };
        }
        None => {
            // Viewing availability by date
            if let Some(selected_date) = selected_date {
                match display_date_availability(
                    bot,
                    dialogue.chat_id(),
                    username,
                    &selected_date,
                    &availability_list,
                    &role_type,
                    &prefix,
                    start,
                    show,
                    Some(msg_id),
                ).await {
                    Ok(_) => {
                        log::debug!("Transition to PlanView (viewing by date) with MsgId: {:?}, Date: {:?}, Start: {:?}", msg_id, selected_date, start);
                        dialogue.update(State::PlanView {
                            msg_id,
                            user_details: None,
                            selected_date: Some(selected_date),
                            availability_list,
                            role_type,
                            prefix,
                            start
                        }).await?;
                    }
                    Err(_) => dialogue.update(State::ErrorState).await?,
                };
            } else {
                dialogue.update(State::ErrorState).await?;
            }
        }
    }

    Ok(())
}

pub(super) async fn plan(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    ops_name_or_date: String,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "plan", "Command", msg);

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Get the user in the database to determine their role
    let query_user_details = match controllers::user::get_user_by_tele_id(&pool, user.id.0).await {
        Ok(user) => user,
        Err(_) => {
            handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
            return Ok(())
        },
    };

    // Generate random prefix to make the IDs only applicable to this dialogue instance
    let prefix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect();

    // Try to interpret the argument as an OPS NAME first
    match controllers::user::user_exists_ops_name(&pool, ops_name_or_date.as_ref()).await{
        Ok(exists) => {
            if exists {
                match controllers::user::get_user_by_ops_name(&pool, ops_name_or_date.as_ref()).await {
                    Ok(user_details) => {
                        // show the dates for which the user is available
                        // Get the user's tele_id
                        let tele_id = user_details.tele_id as u64;

                        match controllers::scheduling::get_upcoming_availability_details_by_tele_id(&pool, tele_id).await {
                            Ok(availability_list) => {
                                // Display the user's availability
                                handle_show_avail_by_user(
                                    &bot, &dialogue, &user.username,
                                    user_details,
                                    availability_list,
                                    query_user_details.role_type,
                                    prefix, 0, 8,
                                ).await?;
                            }
                            Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                        }
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                }
            } else {
                // unable to interpret as OPS NAME, interpreting as date
                match utils::parse_single_date(ops_name_or_date.as_ref()) {
                    Some(selected_date) => {
                        // show the available users on that day
                        let today = chrono::Local::now().naive_local().date();
                        if selected_date < today {
                            send_msg(
                                bot.send_message(
                                    dialogue.chat_id(),
                                    "Please provide a date that is today or in the future.",
                                ),
                                &user.username,
                            )
                                .await;
                            dialogue.update(State::Start).await?;
                            return Ok(());
                        }
                        // Show the available users on that day
                        match controllers::scheduling::get_users_available_by_role_on_date(&pool, selected_date, &query_user_details.role_type).await {
                            Ok(availability_list) => {
                                // Display the availability for the selected date
                                handle_show_avail_by_date(
                                    &bot, &dialogue, &user.username,
                                    selected_date,
                                    availability_list,
                                    query_user_details.role_type,
                                    prefix, 0, 8
                                ).await?;
                            }
                            Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                        }
                    }
                    None => {
                        // Neither an OPS NAME nor a valid date
                        send_msg(
                            bot.send_message(
                                dialogue.chat_id(),
                                "Invalid input. Please provide a valid OPS NAME or date.",
                            ),
                            &user.username,
                        )
                            .await;
                        dialogue.update(State::Start).await?;
                    }
                }
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }

    Ok(())
}

pub(super) async fn plan_view(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    (
        msg_id,
        user_details,
        selected_date,
        availability_list,
        role_type,
        prefix,
        start
    ): (
        MessageId,
        Option<Usr>,
        Option<NaiveDate>,
        Vec<AvailabilityDetails>,
        RoleType,
        String,
        usize
    ),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "plan_view", "Callback", q,
        "MsgId" => msg_id,
        "UserDetails" => user_details,
        "SelectedDate" => selected_date,
        "AvailabilityList" => availability_list,
        "RoleType" => role_type,
        "Prefix" => prefix,
        "Start" => start
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            handle_re_show_options(
                &bot, &dialogue, &q.from.username,
                user_details,
                selected_date,
                availability_list,
                role_type,
                prefix, start, 8,
                msg_id
            ).await?;
        }
        Some(option) => {
            if option == "PREV" {
                handle_re_show_options(
                    &bot, &dialogue, &q.from.username,
                    user_details,
                    selected_date,
                    availability_list,
                    role_type,
                    prefix, max(0, start-8), 8,
                    msg_id
                ).await?;
            } else if option == "NEXT" {
                let entries_len = availability_list.len();
                handle_re_show_options(
                    &bot, &dialogue, &q.from.username,
                    user_details,
                    selected_date,
                    availability_list,
                    role_type,
                    prefix, if start+8 < entries_len { start+8 } else { start }, 8,
                    msg_id
                ).await?;
            } else if option == "DONE" {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Done."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else {
                match option.strip_prefix(&prefix) {
                    Some(id) => {
                        match Uuid::try_parse(&id) {
                            Ok(parsed_avail_uuid) => {
                                // plan or unplan users
                                // if currently planned -> unplan user
                                // if currently unplanned -> plan user
                                match controllers::scheduling::toggle_planned_status(
                                    &pool,
                                    parsed_avail_uuid,

                                ).await {
                                    Ok(availability_details) => {
                                        // notify planned
                                        notifier::emit::plan_notifications(
                                            &bot,
                                            format!(
                                                "{}{} has been planned for {} on {}",
                                                availability_details.ops_name,
                                                if availability_details.usr_type == UsrType::NS {" (NS)"} else {""},
                                                availability_details.ict_type.as_ref(),
                                                availability_details.avail.format("%Y-%m-%d")
                                            ).as_str(),
                                            &pool,
                                        ).await;

                                        send_msg(
                                            bot.send_message(dialogue.chat_id(), format!(
                                                "{} {} for {}",
                                                availability_details.ops_name,
                                                if availability_details.planned { "planned" } else { "no longer planned" },
                                                availability_details.avail.format("%b %d, %Y")
                                            )),
                                            &q.from.username,
                                        ).await;

                                        let new_availability_list = match user_details {
                                            Some(ref user_details) => {
                                                // by user
                                                // Get the user's tele_id
                                                let tele_id = user_details.tele_id as u64;

                                                match controllers::scheduling::get_upcoming_availability_details_by_tele_id(&pool, tele_id).await {
                                                    Ok(list) => list,
                                                    Err(_) => {
                                                        dialogue.update(State::ErrorState).await?;
                                                        return Ok(());
                                                    }
                                                }
                                            }
                                            None => {
                                                match selected_date {
                                                    Some(selected_date) => {
                                                        // by date

                                                        match controllers::scheduling::get_users_available_by_role_on_date(&pool, selected_date, &role_type).await {
                                                            Ok(list) => list,
                                                            Err(_) => {
                                                                dialogue.update(State::ErrorState).await?;
                                                                return Ok(());
                                                            }
                                                        }
                                                    }
                                                    None => {
                                                        dialogue.update(State::ErrorState).await?;
                                                        return Ok(());
                                                    }
                                                }
                                            }
                                        };

                                        let newstart = if start < availability_list.len()-1 { start } else { max(start-8, 0) };
                                        handle_re_show_options(
                                            &bot, &dialogue, &q.from.username,
                                            user_details,
                                            selected_date,
                                            new_availability_list,
                                            role_type,
                                            prefix, newstart, 8,
                                            msg_id
                                        ).await?;
                                    }
                                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                                }
                            }
                            Err(_) => {
                                send_msg(
                                    bot.send_message(dialogue.chat_id(), "Invalid option."),
                                    &q.from.username,
                                ).await;
                                handle_re_show_options(
                                    &bot, &dialogue, &q.from.username,
                                    user_details,
                                    selected_date,
                                    availability_list,
                                    role_type,
                                    prefix, start, 8,
                                    msg_id
                                ).await?;
                            }
                        }
                    }
                    None => {
                        // change role logic
                        match RoleType::from_str(&option) {
                            Ok(role_type_enum) => {
                                match selected_date {
                                    Some(selected_date) => {
                                        // Show the available users on that day
                                        match controllers::scheduling::get_users_available_by_role_on_date(&pool, selected_date, &role_type_enum).await {
                                            Ok(new_availability_list) => {
                                                // Display the availability for the selected date
                                                handle_re_show_options(
                                                    &bot, &dialogue, &q.from.username,
                                                    None,
                                                    Some(selected_date),
                                                    new_availability_list,
                                                    role_type_enum,
                                                    prefix, 0, 8,
                                                    msg_id
                                                ).await?;
                                            }
                                            Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                                        }
                                    }
                                    None => {
                                        send_msg(
                                            bot.send_message(dialogue.chat_id(), "Invalid option."),
                                            &q.from.username,
                                        ).await;
                                        handle_re_show_options(
                                            &bot, &dialogue, &q.from.username,
                                            user_details,
                                            selected_date,
                                            availability_list,
                                            role_type,
                                            prefix, start, 8,
                                            msg_id
                                        ).await?;
                                    }
                                }
                            }
                            Err(_) => {
                                send_msg(
                                    bot.send_message(dialogue.chat_id(), "Invalid option."),
                                    &q.from.username,
                                ).await;
                                handle_re_show_options(
                                    &bot, &dialogue, &q.from.username,
                                    user_details,
                                    selected_date,
                                    availability_list,
                                    role_type,
                                    prefix, start, 8,
                                    msg_id
                                ).await?;
                            }
                        }
                    }
                }
            }
        }
    }


    Ok(())
}