use chrono::{Datelike, Duration, Local, NaiveDate};

use sqlx::PgPool;

use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

use crate::bot::state::State;
use crate::bot::{handle_error, log_try_delete_msg, log_try_remove_markup, match_callback_data, retrieve_callback_data, send_msg, HandlerResult, MyDialogue};
use crate::types::{AvailabilityDetails, RoleType, UsrType};
use crate::{controllers, log_endpoint_hit, utils};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumProperty;
use callback_data::CallbackData;
use callback_data::CallbackDataHandler;

// Represents callback actions with optional associated data.
#[derive(Debug, Clone, Serialize, Deserialize, EnumProperty, CallbackData)]
enum ForecastCallbackData {
    // View range Actions
    ViewNextWeek,
    ViewThisMonth,
    ViewNextMonth,
    IncNextMonth,

    // Select Role Actions
    ChangeRole { role_type: RoleType },

    // Completion Action
    Done
}

async fn display_availability_forecast(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    role_type: &RoleType,
    availability_list: &Vec<AvailabilityDetails>,
    start: NaiveDate,
    end: NaiveDate,
    prefix: &String,
    edit_msg: Option<MessageId>
) -> Option<MessageId> {
    let change_view_roles: Vec<InlineKeyboardButton> = RoleType::iter()
        .filter_map(|role| { if *role_type != role {
                Some(InlineKeyboardButton::callback(
                    "VIEW ".to_owned() + role.clone().as_ref(),
                    ForecastCallbackData::ChangeRole { role_type: role }.to_callback_data(&prefix)
                ))
            } else { None }
        })
        .collect();

    let mut view_range: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let this_month_str = Local::now().format("%b").to_string().to_uppercase();
    let next_month_str = utils::add_month_safe(Local::now().date_naive(), 1).format("%b").to_string().to_uppercase();
    view_range.push(
        [("NEXT WEEK", ForecastCallbackData::ViewNextWeek), (&this_month_str, ForecastCallbackData::ViewThisMonth), (&next_month_str, ForecastCallbackData::ViewNextMonth)]
            .into_iter()
            .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(&prefix)))
            .collect(),
    );
    view_range.push(
        [("DONE", ForecastCallbackData::Done), ("+1 MONTH", ForecastCallbackData::IncNextMonth)]
            .into_iter()
            .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(&prefix)))
            .collect(),
    );

    let mut options: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    options.push(change_view_roles);
    options.extend(view_range);

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

    // Calculate the length of the longest `ops_name`
    let max_len = availability_list.iter()
        .map(|info| info.ops_name.len())
        .max()
        .unwrap_or(0); // Handle case when result is empty

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

    let today = Local::now().date_naive();
    // Format the availability information grouped by year and month
    for ((year, month), availabilities) in availability_by_year_month {
        let date_month = NaiveDate::from_ymd_opt(year, month, 1)?;
        let year_month_str = format!("*{} {}*\n", date_month.format("%B"), year); // Display month and year
        let cancel_if_past_str = if utils::last_day_of_month(date_month) < today { format!("~{}~", year_month_str).to_string() } else { year_month_str };
        output_text.push_str(&cancel_if_past_str);

        let mut availability_by_date: std::collections::BTreeMap<NaiveDate, Vec<&AvailabilityDetails>> = std::collections::BTreeMap::new();
        for availability in availabilities {
            availability_by_date
                .entry(availability.avail)
                .or_insert(Vec::new())
                .push(availability);
        }

        // Format availability for each day in the month
        for (date, availabilities_for_day) in availability_by_date {
            let day_str = format!("__{}__\n", date.format("%b %d")); // Format as "Sep 05"
            let cancel_if_day_past_str = if date < today { format!("~{}~", day_str).to_string() } else { day_str };
            output_text.push_str(&cancel_if_day_past_str);
            
            let mut per_day: String;

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

                // Truncate remarks
                let remarks_str = if let Some(remarks) = &availability.remarks {
                    if remarks.chars().count() > utils::MAX_REMARKS_SHOWN_CHARS_TEXT {
                        format!(
                            "{}: {}\\.\\.\\.",
                            saf100_str,
                            utils::escape_special_characters(remarks.chars().take(utils::MAX_REMARKS_SHOWN_CHARS_TEXT).collect::<String>().as_str())
                        )
                    } else {
                        format!("{}: {}", saf100_str, utils::escape_special_characters(&remarks))
                    }
                } else {
                    saf100_str.to_string()
                };

                per_day = format!(
                    "\\- `{:<width$}` {} __{}__{}{}{}\n",
                    availability.ops_name,
                    usrtype_str,
                    availability.ict_type.as_ref(),
                    planned_str,
                    avail,
                    remarks_str,
                    width = max_len
                );

                if availability.avail < Local::now().date_naive() {
                    output_text.push_str(&format!("~{}~", per_day));
                } else {
                    output_text.push_str(&per_day);
                }
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
                    let prefix = utils::generate_prefix(utils::CALLBACK_PREFIX_LEN);
                    match display_availability_forecast(&bot, dialogue.chat_id(), &user.username, &role_type, &availability_list, start, end, &prefix, None).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(msg_id) => dialogue.update(State::ForecastView { msg_id, prefix, availability_list, role_type, start, end }).await?
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
    (msg_id, prefix, availability_list, role_type, start, end): (MessageId, String, Vec<AvailabilityDetails>, RoleType, NaiveDate, NaiveDate),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "forecast_view", "Callback", q,
        "MessageId" => msg_id,
        "Prefix" => prefix,
        "AvailabilityList" => availability_list,
        "RoleType" => role_type,
        "Start" => start,
        "End" => end
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

    let mut new_role = role_type.clone();
    let mut new_start = start;
    let mut new_end = end;

    match callback {
        ForecastCallbackData::ChangeRole { role_type } => {
            new_role = role_type;
        }
        ForecastCallbackData::Done => {
            if availability_list.is_empty() {
                // Delete the existing message if no availability is shown
                log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
            } else {
                // Edit the existing message to remove the inline keyboard
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            }
            dialogue.update(State::Start).await?;
            return Ok(());
        }
        ForecastCallbackData::ViewNextWeek => {
            new_start = Local::now().date_naive();
            new_end = end.checked_add_signed(Duration::weeks(1)).expect("Overflow when adding duration");
        }
        ForecastCallbackData::ViewThisMonth => {
            let (this_month_start, this_month_end) = utils::this_month_bound();
            new_start = this_month_start;
            new_end = this_month_end;
        }
        ForecastCallbackData::ViewNextMonth => {
            let (this_month_start, this_month_end) = utils::this_month_bound();
            new_start = utils::add_month_safe(this_month_start, 1);
            new_end = utils::add_month_safe(this_month_end, 1);
        }
        ForecastCallbackData::IncNextMonth => {
            new_start = utils::add_month_safe(new_start, 1);
            new_end = utils::add_month_safe(new_end, 1);
        }
    }

    match controllers::scheduling::get_availability_for_role_and_dates(&pool, new_role.clone(), new_start, new_end).await {
        Ok(availability_list_new) => {
            match display_availability_forecast(&bot, dialogue.chat_id(), &q.from.username, &new_role, &availability_list_new, new_start, new_end, &prefix, Some(msg_id)).await {
                None => {}
                Some(new_msg_id) => dialogue.update(State::ForecastView { msg_id: new_msg_id, prefix, availability_list: availability_list_new, role_type: new_role, start: new_start, end: new_end }).await?
            };
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
    }
    
    Ok(())
}