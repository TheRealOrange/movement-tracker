use sqlx::PgPool;
use teloxide::prelude::*;
use crate::controllers;

async fn send_helper(bot: &Bot, chats_to_send: Vec<i64>, message: &str) {
    for chat in chats_to_send {
        let chat_id = ChatId(chat);

        if let Err(e) = bot.send_message(chat_id.clone(), message).await {
            log::error!("Failed to send message to chat_id {}: {}", chat, e);
        } else {
            log::info!("Successfully sent message to chat_id {}.", chat);
        }
    }
}

pub(crate) async fn system_notifications(bot: &Bot, message: &str, pool: &PgPool) {
    match controllers::notifications::get_system_notifications_enabled(pool).await {
        Ok(chats) => {
            if chats.is_empty() {
                log::info!("No system notifications enabled for any chat.");
                return;
            }

            log::info!("Sending system notifications to {} chats. Message: {}", chats.len(), message);
            send_helper(bot, chats, message).await;
        }
        Err(e) => {
            log::error!("Failed to retrieve system notification settings: {}", e);
        }
    }
}

pub(crate) async fn register_notifications(bot: &Bot, message: &str, pool: &PgPool) {
    match controllers::notifications::get_register_notifications_enabled(pool).await {
        Ok(chats) => {
            if chats.is_empty() {
                log::info!("No register notifications enabled for any chat.");
                return;
            }

            log::info!("Sending register notifications to {} chats. Message: {}", chats.len(), message);
            send_helper(bot, chats, message).await;
        }
        Err(e) => {
            log::error!("Failed to retrieve register notification settings: {}", e);
        }
    }
}

pub(crate) async fn availability_notifications(bot: &Bot, message: &str, pool: &PgPool) {
    match controllers::notifications::get_availability_notifications_enabled(pool).await {
        Ok(chats) => {
            if chats.is_empty() {
                log::info!("No availability notifications enabled for any chat.");
                return;
            }

            log::info!("Sending availability notifications to {} chats. Message: {}", chats.len(), message);
            send_helper(bot, chats, message).await;
        }
        Err(e) => {
            log::error!("Failed to retrieve availability notification settings: {}", e);
        }
    }
}

pub(crate) async fn plan_notifications(bot: &Bot, message: &str, pool: &PgPool) {
    match controllers::notifications::get_plan_notifications_enabled(pool).await {
        Ok(chats) => {
            if chats.is_empty() {
                log::info!("No plan notifications enabled for any chat.");
                return;
            }

            log::info!("Sending plan notifications to {} chats. Message: {}", chats.len(), message);
            send_helper(bot, chats, message).await;
        }
        Err(e) => {
            log::error!("Failed to retrieve plan notification settings: {}", e);
        }
    }
}

pub(crate) async fn conflict_notifications(bot: &Bot, message: &str, pool: &PgPool) {
    match controllers::notifications::get_conflict_notifications_enabled(pool).await {
        Ok(chats) => {
            if chats.is_empty() {
                log::info!("No conflict notifications enabled for any chat.");
                return;
            }

            log::info!("Sending conflict notifications to {} chats. Message: {}", chats.len(), message);
            send_helper(bot, chats, message).await;
        }
        Err(e) => {
            log::error!("Failed to retrieve conflict notification settings: {}", e);
        }
    }
}