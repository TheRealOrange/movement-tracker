use std::cmp::{max, min};
use std::str::FromStr;
use chrono::NaiveDate;
use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::{Error, PgPool};
use strum::IntoEnumIterator;
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::{CallbackQuery, ChatId, Message, Requester};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyParameters};
use uuid::Uuid;
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit, utils};
use crate::bot::state::State::AvailabilitySelect;
use crate::bot::user::user;
use crate::types::{Apply, Availability, AvailabilityDetails, RoleType, Usr, UsrType};
use super::{handle_error, send_msg, HandlerResult, MyDialogue};

// Helper function to display a user's availability
async fn display_user_availability(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    user_details: &Usr,
    availability_list: &Vec<AvailabilityDetails>,
    role_type: &RoleType,
    prefix: &String,
    start: usize,
    show: usize,
) -> Result<(), ()> {
    let slice_end = min(start+show, availability_list.len()-1);
    let shown_entries = if let Some(shown_entries) = availability_list.get(start..slice_end+1) {
        shown_entries
    } else {
        log::error!("Cannot get availability entries slice");
        send_msg(
            bot.send_message(chat_id, "Error encountered while getting availability"),
            username
        ).await;
        return Err(());
    };
    
    let change_view_roles: Vec<InlineKeyboardButton> = RoleType::iter()
        .filter_map(|role| if *role_type != role { Some(InlineKeyboardButton::callback("VIEW ".to_owned() + role.as_ref(), role.as_ref())) } else { None })
        .collect();

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries.into_iter()
        .filter_map(|entry| {
            let option_str = if entry.planned { if entry.is_valid { "UNPLAN" } else { "UNPLAN \\*(UNAVAIL\\)*"}} else { "PLAN" };
            let truncated_remarks = if let Some(remarks) = &entry.remarks {
                if remarks.len() > 8 {
                    format!(", {}...", &remarks[0..8])
                } else {
                    format!(", {}", &remarks)
                }
            } else {
                "".to_string()
            };
            // Format date as "MMM-DD" (3-letter month)
            let formatted = format!(
                "{} {}: {}{}", 
                option_str,
                entry.avail.format("%b-%d").to_string(), 
                entry.ict_type.as_ref(), 
                truncated_remarks
            );
            if entry.is_valid {
                Some(vec![InlineKeyboardButton::callback(formatted, prefix.to_owned() + &entry.id.to_string())])
            } else {
                None
            }
        })
        .collect();

    // Add "NEXT" and "PREV" buttons
    let mut pagination = Vec::new();
    if start != 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", "PREV"));
    }
    if slice_end != availability_list.len()-1 {
        pagination.push(InlineKeyboardButton::callback("NEXT", "NEXT"));
    }
    pagination.push(InlineKeyboardButton::callback("DONE", "DONE"));

    entries.push(change_view_roles);
    entries.push(pagination);
    
    let mut message = format!(
        "Availability for {}:\n",
        utils::escape_special_characters(&user_details.ops_name)
    );

    if availability_list.is_empty() {
        message.push_str("No upcoming availability.\n");
    } else {
        for availability in availability_list {
            let date_str = availability.avail.format("%b %d, %Y").to_string();
            let ict_type_str = availability.ict_type.as_ref();
            let planned_str = if availability.planned { " \\(PLANNED\\)" } else { "" };
            let avail_str = if availability.is_valid { ""} else { " *\\(UNAVAIL\\)*" };
            let usrtype_str = if availability.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
            let saf100_str = if availability.saf100 { " SAF100 ISSUED"} else { if availability.planned && availability.usr_type == UsrType::NS { " *PENDING SAF100*" } else { "" } };

            // Truncate remarks to a max of 15 characters
            let remarks_str = if let Some(remarks) = &availability.remarks {
                if remarks.len() > 15 {
                    format!("{}: {}\\.\\.\\.", saf100_str, utils::escape_special_characters(&remarks[0..15]))
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

    send_msg(
        bot.send_message(chat_id, message)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(InlineKeyboardMarkup::new(entries)),
        username,
    ).await;
    
    Ok(())
}

// Helper function to display availability for a specific date
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
) -> Result<(), ()> {
    let slice_end = min(start+show, availability_list.len()-1);
    let shown_entries = if let Some(shown_entries) = availability_list.get(start..slice_end+1) {
        shown_entries
    } else {
        log::error!("Cannot get availability entries slice");
        send_msg(
            bot.send_message(chat_id, "Error encountered while getting availability"),
            username
        ).await;
        return Err(());
    };

    let change_view_roles: Vec<InlineKeyboardButton> = RoleType::iter()
        .filter_map(|role| if *role_type != role { Some(InlineKeyboardButton::callback("VIEW ".to_owned() + role.as_ref(), role.as_ref())) } else { None })
        .collect();

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries.into_iter()
        .filter_map(|entry| {
            let option_str = if entry.planned { if entry.is_valid { "UNPLAN" } else { "UNPLAN \\*(UNAVAIL\\)*"} } else { "PLAN" };
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
            if entry.is_valid {
                Some(vec![InlineKeyboardButton::callback(formatted, prefix.to_owned() + &entry.id.to_string())])
            } else {
                None
            }
        })
        .collect();

    // Add "NEXT" and "PREV" buttons
    let mut pagination = Vec::new();
    if start != 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", "PREV"));
    }
    if slice_end != availability_list.len()-1 {
        pagination.push(InlineKeyboardButton::callback("NEXT", "NEXT"));
    }
    pagination.push(InlineKeyboardButton::callback("DONE", "DONE"));

    entries.push(change_view_roles);
    entries.push(pagination);
    
    let date_str = selected_date.format("%b %d, %Y").to_string();
    let mut message = format!(
        "Users available on {}:\n",
        utils::escape_special_characters(&date_str)
    );

    if availability_list.is_empty() {
        message.push_str("No users available on this date.\n");
    } else {
        for availability in availability_list {
            let ict_type_str = availability.ict_type.as_ref();
            let planned_str = if availability.planned { " \\(PLANNED\\)" } else { "" };
            let avail_str = if availability.is_valid { ""} else { " *\\(UNAVAIL\\)*" };
            let usrtype_str = if availability.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
            let saf100_str = if availability.saf100 { " SAF100 ISSUED"} else { if availability.planned && availability.usr_type == UsrType::NS { " *PENDING SAF100*" } else { "" } };

            // Truncate remarks to a max of 15 characters
            let remarks_str = if let Some(remarks) = &availability.remarks {
                if remarks.len() > 15 {
                    format!("{}: {}\\.\\.\\.", saf100_str, utils::escape_special_characters(&remarks[0..15]))
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

    send_msg(
        bot.send_message(chat_id, message)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(InlineKeyboardMarkup::new(entries)),
        username,
    ).await;
    
    Ok(())
}

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
    show: usize
) -> HandlerResult {
    match user_details {
        Some(user_details) => {
            // user provided, assuming we are viewing availability by user
            match display_user_availability(
                &bot,
                dialogue.chat_id(),
                username,
                &user_details,
                &availability_list,
                &role_type,
                &prefix,
                start,
                show
            ).await {
                Ok(_) => {
                    log::debug!("Transitioning to PlanView (viewing by user) User: {:?}, AvailabilityList: {:?}, Prefix: {:?}, Start: {:?}", user_details, availability_list, prefix, start);
                    dialogue.update(State::PlanView { user_details: Some(user_details), selected_date: None, availability_list, role_type, prefix, start }).await?;
                }
                Err(_) => dialogue.update(State::ErrorState).await?,
            };
        }
        None => {
            match selected_date {
                Some(selected_date) => {
                    // date provided, assuming we are viewing availability by date
                    match display_date_availability(
                        &bot,
                        dialogue.chat_id(),
                        username,
                        &selected_date,
                        &availability_list,
                        &role_type,
                        &prefix,
                        start,
                        show
                    ).await {
                        Ok(_) => {
                            log::debug!("Transitioning to PlanView (viewing by date) SelectedDate: {:?}, AvailabilityList: {:?}, Prefix: {:?}, Start: {:?}", selected_date, availability_list, prefix, start);
                            dialogue.update(State::PlanView { user_details: None, selected_date: Some(selected_date), availability_list, role_type, prefix, start }).await?;
                        }
                        Err(_) => dialogue.update(State::ErrorState).await?,
                    };
                }
                None => dialogue.update(State::ErrorState).await?,
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
                                handle_re_show_options(
                                    &bot, &dialogue, &user.username, 
                                    Some(user_details), 
                                    None, 
                                    availability_list, 
                                    query_user_details.role_type, 
                                    prefix, 0, 8
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
                        match controllers::scheduling::get_users_available_on_date(&pool, selected_date).await {
                            Ok(availability_list) => {
                                // Display the availability for the selected date
                                handle_re_show_options(&bot, &dialogue, &user.username, None, Some(selected_date), availability_list, query_user_details.role_type, prefix, 0, 8).await?;
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
        user_details, 
        selected_date, 
        availability_list, 
        role_type, 
        prefix, 
        start
    ): (
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
                prefix, start, 8
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
                    prefix, max(0, start-8), 8
                ).await?;
            } else if option == "NEXT" {
                let entries_len = availability_list.len();
                handle_re_show_options(
                    &bot, &dialogue, &q.from.username,
                    user_details,
                    selected_date,
                    availability_list,
                    role_type,
                    prefix, if start+8 < entries_len { start+8 } else { start }, 8
                ).await?;
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
                                        send_msg(
                                            bot.send_message(dialogue.chat_id(), format!(
                                                "{} {} for {}",
                                                availability_details.ops_name,
                                                if availability_details.planned { "planned" } else { "no longer planned" },
                                                availability_details.avail.format("%b %d, %Y")
                                            )),
                                            &q.from.username,
                                        ).await;
                                        dialogue.update(State::Start);
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
                                    prefix, start, 8
                                ).await?;
                            }
                        }
                    }
                    None => {
                        // change role logic
                        match RoleType::from_str(&option) {
                            Ok(role_type_enum) => {
                                handle_re_show_options(
                                    &bot, &dialogue, &q.from.username,
                                    user_details,
                                    selected_date,
                                    availability_list,
                                    role_type_enum,
                                    prefix, start, 8
                                ).await?;
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
                                    prefix, start, 8
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