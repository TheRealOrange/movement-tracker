// implement functions used to provide a week forecast, a month forecast

use std::ops::Add;
use std::str::FromStr;
use chrono::{Datelike, Duration, Local, NaiveDate, Utc};
use sqlx::{Error, PgPool};
use strum::{IntoEnumIterator, ParseError};
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::{CallbackQuery, ChatId, Message};
use teloxide::requests::Requester;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyParameters};
use crate::bot::{handle_error, send_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit, utils};
use crate::types::{Availability, AvailabilityDetails, RoleType, UsrType};

async fn display_availability_forecast(bot: &Bot, chat_id: ChatId, username: &Option<String>, role_type: RoleType, availability_list: &Vec<AvailabilityDetails>, start: NaiveDate, end: NaiveDate) {
    let change_view_roles: Vec<InlineKeyboardButton> = RoleType::iter()
        .filter_map(|role| if role_type != role { Some(InlineKeyboardButton::callback("VIEW ".to_owned() + role.as_ref(), role.as_ref())) } else { None })
        .collect();

    let mut view_range: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    view_range.push(
        ["NEXT WEEK", "1 MONTH"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect()
    );
    view_range.push(
        ["2 MONTHS", "VIEW ALL"]
            .into_iter()
            .map(|option| InlineKeyboardButton::callback(option, option))
            .collect()
    );

    let mut options: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    options.push(change_view_roles);
    options.extend(view_range);
    options.push(vec![InlineKeyboardButton::callback("DONE", "DONE")]);

    // Header for role type and period with formatted dates
    let mut output_text = format!(
        "*Availability forecast for role:* __{}__ *from* __{}__ *to* __{}__\n\n",
        role_type.as_ref(),
        start.format("%b\\-%d\\-%Y"),  // Formatting like "Sep 05"
        end.format("%b\\-%d\\-%Y")     // Formatting like "Oct 10"
    );

    // Organize availability by day
    let mut availability_by_date: std::collections::BTreeMap<NaiveDate, Vec<&AvailabilityDetails>> = std::collections::BTreeMap::new();
    for availability in availability_list {
        availability_by_date
            .entry(availability.avail)
            .or_insert(Vec::new())
            .push(availability);
    }

    // Format the availability information for each date
    for (date, availabilities) in availability_by_date {
        output_text.push_str(&format!("__{}__\n", date.format("%b %d"))); // Format as "Sep 05"

        for availability in availabilities {
            let planned_str = if availability.planned { " \\(PLANNED\\)" } else { "" };
            let avail = if !availability.is_valid { " *\\(UNAVAIL\\)*" } else { "" };
            let usrtype_str = if availability.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
            let saf100_str = if availability.saf100 { " SAF100 ISSUED" } else { "" };

            // Truncate remarks to a max of 10 characters
            let remarks_str = if let Some(remarks) = &availability.remarks {
                if remarks.len() > 15 {
                    format!("{}: {}...", saf100_str, &remarks[0..15])
                } else {
                    format!("{}: {}", saf100_str, remarks)
                }
            } else {
                saf100_str.to_string()
            };
            
            let escaped_remarks = utils::escape_special_characters(&remarks_str);

            output_text.push_str(&format!(
                "\\- {} __{}__{}{}{}{}\n",
                availability.ops_name,
                availability.ict_type.as_ref(),
                planned_str,
                avail,
                usrtype_str,
                escaped_remarks
            ));
        }
        output_text.push('\n'); // Add space between dates
    }

    send_msg(
        bot.send_message(chat_id, output_text)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(InlineKeyboardMarkup::new(options)),
        username
    ).await;
}


pub(super) async fn forecast(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "forecast", "Command", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Get the user in the database
    match controllers::user::get_user_by_tele_id(&pool, user.id.0).await{
        Ok(retrieved_user) => {
            // transition to showing the availability for the next week first, with options to view subsequent weeks, months, or whole month
            let role_type = retrieved_user.role_type;
            let start = Local::now().date_naive(); // Get today's date in the local timezone
            let end = start.checked_add_signed(Duration::weeks(1)).expect("Overflow when adding duration");
            match controllers::scheduling::get_availability_for_role_and_dates(&pool, role_type.clone(), start, end).await {
                Ok(availability_list) => {
                    display_availability_forecast(&bot, dialogue.chat_id(), &user.username, role_type.clone(), &availability_list, start, end).await;
                    dialogue.update(State::ForecastView { availability_list, role_type, start, end }).await?;
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
            }
        },
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }

    Ok(())
}

pub(super) async fn forecast_view(
    bot: Bot,
    dialogue: MyDialogue,
    (availability_list, role_type, start, end): (Vec<AvailabilityDetails>, RoleType, NaiveDate, NaiveDate),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "register_role", "Callback", q);

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            display_availability_forecast(&bot, dialogue.chat_id(), &q.from.username, role_type.clone(), &availability_list, start, end).await;
        }
        Some(option) => {
            let mut new_role = role_type.clone();
            let mut new_start = start;
            let mut new_end = end;
            if option == "NEXT WEEK" {
                new_start = start.checked_add_signed(Duration::weeks(1)).expect("Overflow when adding duration");
                new_end = end.checked_add_signed(Duration::weeks(1)).expect("Overflow when adding duration");
            } else if option == "1 MONTH" {
                new_start = Local::now().date_naive(); // Get today's date in the local timezone
                // Add one month
                new_end = utils::add_month_safe(start, 1);
            } else if option == "2 MONTHS" {
                new_start = Local::now().date_naive(); // Get today's date in the local timezone
                // Add one month
                new_end = utils::add_month_safe(start, 2);
            } else if option == "VIEW ALL" {
                match controllers::scheduling::get_furthest_avail_date_for_role(&pool, &role_type).await {
                    Ok(last_date) => {
                        new_start = Local::now().date_naive(); // Get today's date in the local timezone
                        new_end = last_date.unwrap_or(end);
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                }
            } else if option == "DONE" {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Returning to start."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?;
                return Ok(());
            } else {
                match RoleType::from_str(&option) {
                    Ok(input_role_enum) => {
                        new_role = input_role_enum;
                    }
                    Err(_) => {
                        display_availability_forecast(&bot, dialogue.chat_id(), &q.from.username, role_type.clone(), &availability_list, start, end).await;
                        return Ok(());
                    }
                }
            }

            match controllers::scheduling::get_availability_for_role_and_dates(&pool, new_role.clone(), new_start, new_end).await {
                Ok(availability_list_new) => {
                    display_availability_forecast(&bot, dialogue.chat_id(), &q.from.username, new_role.clone(), &availability_list_new, new_start, new_end).await;
                    dialogue.update(State::ForecastView { availability_list: availability_list_new, role_type: new_role, start: new_start, end: new_end }).await?;
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
            }
        }
    }
    
    Ok(())
}