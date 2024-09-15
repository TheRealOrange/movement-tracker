use sqlx::PgPool;
use teloxide::Bot;
use teloxide::prelude::Message;
use enum_iterator::all;
use teloxide::dispatching::dialogue::InMemStorageError;
use teloxide::payloads::SendMessageSetters;
use teloxide::requests::Requester;
use teloxide::types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup};
use crate::bot::state::State;
use crate::controllers;
use crate::types::{RoleType, UsrType};
use super::{send_msg, HandlerResult, MyDialogue};


pub(super) async fn register(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from() {
        user
    } else {
        dialogue.update(State::Start).await?;
        return Ok(());
    };

    // Helper function to log and return false on any errors
    let handle_error = || async {
        send_msg(
            bot.send_message(msg.chat.id, "Error occurred accessing the database")
                .reply_to_message_id(msg.id),
            &(user.username)
        ).await;
    };

    // Check if the telegram ID exists in the database
    match controllers::user::user_exists_tele_id(&pool, user.id.0).await{
        Ok(true) => {
            send_msg(
                bot.send_message(msg.chat.id, "You have already registered")
                    .reply_to_message_id(msg.id),
                &(user.username),
            ).await;
            dialogue.update(State::Start).await?;
        },
        Ok(false) => {
            let roles = all::<RoleType>()
                .map(|role| InlineKeyboardButton::callback(&role, &role));

            bot.send_message(msg.chat.id, "I am a:")
                .reply_markup(InlineKeyboardMarkup::new([roles]))
                .await?;

            dialogue.update(State::RegisterRole).await?
        },
        Err(e) => handle_error().await,
    }

    Ok(())
}

pub(super) async fn register_role(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    todo!()
}

pub(super) async fn register_type(
    bot: Bot,
    dialogue: MyDialogue,
    role_type: RoleType,
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    todo!()
}

pub(super) async fn register_ops_name(
    bot: Bot,
    dialogue: MyDialogue,
    role_type: RoleType,
    user_type: UsrType,
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    todo!()
}
