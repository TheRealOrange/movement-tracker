// implement functions used to provide a week forecast, a month forecast

use chrono::NaiveDate;
use sqlx::PgPool;
use strum::IntoEnumIterator;
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::{ChatId, Message};
use teloxide::requests::Requester;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ReplyParameters};
use crate::bot::{send_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit};
use crate::types::{Availability, AvailabilityDetails, RoleType};

async fn display_availability_forecast(bot: &Bot, chat_id: ChatId, username: &Option<String>, role_type: RoleType, avalability_list: Vec<AvailabilityDetails>, start: NaiveDate, end: NaiveDate) {
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
    
    let availability_str: &str = "todo";
    
    // TODO: display availability grouped by day, and indicate if SANS and if 100A issued
    
    send_msg(
        bot.send_message(chat_id, format!(
            "Availability shown for the period {} to {}\n\n{}",
            start.format("%b-%d-%Y"),
            end.format("%b-%d-%Y"),
            availability_str
        ))
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

    // Helper function to log and return false on any errors
    let handle_error = || async {
        send_msg(
            bot.send_message(dialogue.chat_id(), "Error occurred accessing the database")
                .reply_parameters(ReplyParameters::new(msg.id)),
            &user.username
        ).await;
    };

    // Get the user in the database
    match controllers::user::get_user_by_tele_id(&pool, user.id.0).await{
        Ok(user) => {
            // transition to showing the availability for the next week first, with options to view subsequent weeks, months, or whole month
            dialogue.update(State::Start).await?;
        },
        Err(_) => {
            handle_error().await;
            dialogue.update(State::ErrorState).await?;
        },
    }

    Ok(())
}