// implement functions used to provide a week forecast, a month forecast

use crate::bot::state::State;
use crate::bot::{handle_error, log_try_delete_msg, log_try_remove_markup, send_msg, HandlerResult, MyDialogue};
use crate::types::{AvailabilityDetails, RoleType, UsrType};
use crate::{controllers, log_endpoint_hit, utils};
use chrono::{Datelike, Duration, Local, NaiveDate};
use sqlx::PgPool;
use std::str::FromStr;
use strum::IntoEnumIterator;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

async fn display_availability_forecast(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    role_type: &RoleType,
    availability_list: &Vec<AvailabilityDetails>,
    start: NaiveDate,
    end: NaiveDate,
    edit_msg: Option<MessageId>
) -> Option<MessageId> {
    let change_view_roles: Vec<InlineKeyboardButton> = RoleType::iter()
        .filter_map(|role| { if *role_type != role {
                Some(InlineKeyboardButton::callback("VIEW ".to_owned() + role.as_ref(), role.as_ref(), ))
            } else { None }
        })
        .collect();

    let mut view_range: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    view_range.push(
        ["NEXT WEEK", "1 MONTH"]
            .into_iter()
            .map(|option| InlineKeyboardButton::callback(option, option))
            .collect(),
    );
    view_range.push(
        ["2 MONTHS", "VIEW ALL"]
            .into_iter()
            .map(|option| InlineKeyboardButton::callback(option, option))
            .collect(),
    );

    let mut options: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    options.push(change_view_roles);
    options.extend(view_range);
    options.push(vec![InlineKeyboardButton::callback("DONE", "DONE")]);

    // Header for role type and period with formatted dates
    let mut output_text = format!(
        "*{} for role:* __{}__ *from* __{}__ *to* __{}__\n\n",
        if availability_list.is_empty() {
            "No availability entries"
        } else {
            "Availability forecast"
        },
        role_type.as_ref(),
        start.format("%b\\-%d\\-%Y"),
        end.format("%b\\-%d\\-%Y")
    );

    // Group availability by year and month
    let mut availability_by_year_month: std::collections::BTreeMap<(i32, u32), Vec<&AvailabilityDetails>> = std::collections::BTreeMap::new();
    for availability in availability_list {
        let year = availability.avail.year();
        let month = availability.avail.month();
        availability_by_year_month
            .entry((year, month))
            .or_insert(Vec::new())
            .push(availability);
    }

    // Format the availability information grouped by year and month
    for ((year, month), availabilities) in availability_by_year_month {
        output_text.push_str(&format!("**{} {}**\n", NaiveDate::from_ymd_opt(year, month, 1)?.format("%B"), year)); // Display month and year

        let mut availability_by_date: std::collections::BTreeMap<NaiveDate, Vec<&AvailabilityDetails>> = std::collections::BTreeMap::new();
        for availability in availabilities {
            availability_by_date
                .entry(availability.avail)
                .or_insert(Vec::new())
                .push(availability);
        }

        // Format availability for each day in the month
        for (date, availabilities_for_day) in availability_by_date {
            output_text.push_str(&format!("__{}__\n", date.format("%b %d"))); // Format as "Sep 05"

            for availability in availabilities_for_day {
                let planned_str = if availability.planned { " \\(PLANNED\\)" } else { "" };
                let avail = if !availability.is_valid { " *\\(UNAVAIL\\)*" } else { "" };
                let usrtype_str = if availability.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
                let saf100_str = if availability.saf100 {
                    " SAF100 ISSUED"
                } else if availability.planned && availability.usr_type == UsrType::NS {
                    " *PENDING SAF100*"
                } else {
                    ""
                };

                // Truncate remarks to a max of 15 characters
                let remarks_str = if let Some(remarks) = &availability.remarks {
                    if remarks.len() > 15 {
                        format!(
                            "{}: {}\\.\\.\\.",
                            saf100_str,
                            utils::escape_special_characters(&remarks[0..15])
                        )
                    } else {
                        format!("{}: {}", saf100_str, utils::escape_special_characters(&remarks))
                    }
                } else {
                    saf100_str.to_string()
                };

                output_text.push_str(&format!(
                    "\\- {} __{}__{}{}{}{}\n",
                    availability.ops_name,
                    availability.ict_type.as_ref(),
                    planned_str,
                    avail,
                    usrtype_str,
                    remarks_str
                ));
            }
            output_text.push('\n'); // Add space between dates
        }
        output_text.push('\n'); // Add space between months
    }

    output_text.push_str(
        format!(
            "\nUpdated: {}",
            Local::now().format("%d%m %H%M\\.%S").to_string()
        ).as_ref(),
    );

    // Send or edit the message
    match edit_msg {
        Some(id) => {
            // Edit the existing message
            match bot
                .edit_message_text(chat_id, id, output_text.clone())
                .parse_mode(ParseMode::MarkdownV2)
                .reply_markup(InlineKeyboardMarkup::new(options.clone()))
                .await
            {
                Ok(edit_msg) => Some(edit_msg.id),
                Err(e) => {
                    log::error!("Failed to update existing message ({}): {}", id.0, e);
                    send_msg(
                        bot.send_message(chat_id, output_text)
                            .parse_mode(ParseMode::MarkdownV2)
                            .reply_markup(InlineKeyboardMarkup::new(options)),
                        username,
                    )
                        .await
                }
            }
        }
        None => {
            send_msg(
                bot.send_message(chat_id, output_text)
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(InlineKeyboardMarkup::new(options)),
                username,
            )
                .await
        }
    }
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
                    match display_availability_forecast(&bot, dialogue.chat_id(), &user.username, &role_type, &availability_list, start, end, None).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(msg_id) => dialogue.update(State::ForecastView { msg_id, availability_list, role_type, start, end }).await?
                    };
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
    (msg_id, availability_list, role_type, start, end): (MessageId, Vec<AvailabilityDetails>, RoleType, NaiveDate, NaiveDate),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "forecast_view", "Callback", q,
        "MessageId" => msg_id,
        "AvailabilityList" => availability_list,
        "RoleType" => role_type,
        "Start" => start,
        "End" => end
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
                if availability_list.is_empty() {
                    // Delete the existing message if no availability is shown
                    log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                } else {
                    // Edit the existing message to remove the inline keyboard
                    log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                }
                dialogue.update(State::Start).await?;
                return Ok(());
            } else {
                match RoleType::from_str(&option) {
                    Ok(input_role_enum) => {
                        new_role = input_role_enum;
                    }
                    Err(_) => {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "Invalid option."),
                            &q.from.username,
                        ).await;
                        return Ok(());
                    }
                }
            }

            match controllers::scheduling::get_availability_for_role_and_dates(&pool, new_role.clone(), new_start, new_end).await {
                Ok(availability_list_new) => {
                    match display_availability_forecast(&bot, dialogue.chat_id(), &q.from.username, &new_role, &availability_list_new, new_start, new_end, Some(msg_id)).await {
                        None => {}
                        Some(new_msg_id) => dialogue.update(State::ForecastView { msg_id: new_msg_id, availability_list: availability_list_new, role_type: new_role, start: new_start, end: new_end }).await?
                    };
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
            }
        }
    }
    
    Ok(())
}