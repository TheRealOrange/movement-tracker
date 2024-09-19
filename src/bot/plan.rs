use std::str::FromStr;
use chrono::NaiveDate;
use sqlx::{Error, PgPool};
use strum::IntoEnumIterator;
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::{CallbackQuery, ChatId, Message, Requester};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyParameters};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit, utils};
use crate::types::{Apply, Availability, AvailabilityDetails, RoleType, Usr, UsrType};
use super::{handle_error, send_msg, HandlerResult, MyDialogue};

// Helper function to display a user's availability
async fn display_user_availability(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    user_details: &Usr,
    availability_list: &Vec<Availability>,
) {
    let mut message = format!(
        "Availability for {} ({}):\n",
        utils::escape_special_characters(&user_details.name),
        utils::escape_special_characters(&user_details.ops_name)
    );

    if availability_list.is_empty() {
        message.push_str("No upcoming availability.\n");
    } else {
        for availability in availability_list {
            let date_str = availability.avail.format("%b %d, %Y").to_string();
            let ict_type_str = availability.ict_type.as_ref();
            let planned_str = if availability.planned { " (Planned)" } else { "" };
            let remarks_str = if let Some(ref remarks) = availability.remarks {
                format!(" - {}", utils::escape_special_characters(remarks))
            } else {
                "".to_string()
            };

            message.push_str(&format!(
                "• {} - {}{}{}\n",
                utils::escape_special_characters(&date_str),
                utils::escape_special_characters(ict_type_str),
                planned_str,
                remarks_str
            ));
        }
    }

    send_msg(
        bot.send_message(chat_id, message)
            .parse_mode(ParseMode::MarkdownV2),
        username,
    )
        .await;
}

// Helper function to display availability for a specific date
async fn display_date_availability(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    selected_date: NaiveDate,
    availability_list: &Vec<AvailabilityDetails>,
) {
    let date_str = selected_date.format("%b %d, %Y").to_string();
    let mut message = format!(
        "Users available on {}:\n",
        utils::escape_special_characters(&date_str)
    );

    if availability_list.is_empty() {
        message.push_str("No users available on this date.\n");
    } else {
        for availability in availability_list {
            let ops_name = &availability.ops_name;
            let ict_type_str = availability.ict_type.as_ref();
            let planned_str = if availability.planned { " (Planned)" } else { "" };
            let remarks_str = if let Some(ref remarks) = availability.remarks {
                format!(" - {}", utils::escape_special_characters(remarks))
            } else {
                "".to_string()
            };

            message.push_str(&format!(
                "• {} - {}{}{}\n",
                utils::escape_special_characters(ops_name),
                utils::escape_special_characters(ict_type_str),
                planned_str,
                remarks_str
            ));
        }
    }

    send_msg(
        bot.send_message(chat_id, message)
            .parse_mode(ParseMode::MarkdownV2),
        username,
    )
        .await;
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
    let user_details = match controllers::user::get_user_by_tele_id(&pool, user.id.0).await {
        Ok(user) => user,
        Err(_) => {
            handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
            return Ok(())
        },
    };


    // Try to interpret the argument as an OPS NAME first
    match controllers::user::user_exists_ops_name(&pool, ops_name_or_date.as_ref()).await{
        Ok(exists) => {
            if exists {
                match controllers::user::get_user_by_ops_name(&pool, ops_name_or_date.as_ref()).await {
                    Ok(user_details) => {
                        // show the dates for which the user is available
                        // Get the user's tele_id
                        let tele_id = user_details.tele_id as u64;

                        match controllers::scheduling::get_upcoming_availability_by_tele_id(&pool, tele_id).await {
                            Ok(availability_list) => {
                                // Display the user's availability
                                display_user_availability(
                                    &bot,
                                    dialogue.chat_id(),
                                    &user.username,
                                    &user_details,
                                    &availability_list,
                                )
                                    .await;

                                dialogue.update(State::Start).await?;
                            }
                            Err(_) => {
                                handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
                                return Ok(());
                            }
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
                                display_date_availability(
                                    &bot,
                                    dialogue.chat_id(),
                                    &user.username,
                                    selected_date,
                                    &availability_list,
                                )
                                    .await;

                                dialogue.update(State::Start).await?;
                            }
                            Err(_) => {
                                handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
                                return Ok(());
                            }
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
                        return Ok(());
                    }
                }
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }

    Ok(())
}