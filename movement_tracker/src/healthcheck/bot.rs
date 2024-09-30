use std::env;
use std::sync::Arc;
use std::fmt::Debug;
use chrono::Utc;

use futures::future::BoxFuture;

use teloxide::prelude::*;
use teloxide::error_handlers::ErrorHandler;
use teloxide::types::ParseMode;

use crate::{now, AppState};
use crate::APP_TIMEZONE;

// Custom Error Handler Struct
pub(crate) struct HealthCheckErrorHandler {
    pub app_state: Arc<AppState>,
}

impl HealthCheckErrorHandler {
    pub fn new(app_state: Arc<AppState>) -> Arc<Self> {
        Arc::new(Self { app_state })
    }
}

impl<E> ErrorHandler<E> for HealthCheckErrorHandler
where
    E: Debug,
{
    fn handle_error(self: Arc<Self>, error: E) -> BoxFuture<'static, ()> {
        let app_state = self.app_state.clone();
        log::error!("Error in update listener: {:?}", error);
        Box::pin(async move {
            let mut health = app_state.bot_health.lock().await;
            *health = false;
        })
    }
}

// Checks if the bot is healthy by sending a "ping" message to itself
pub(super) async fn check_bot_health(bot: &Bot, app_state: &AppState) -> Option<bool> {
    if !app_state.bot_health_check_active {
        // If bot health check is not active, return None to indicate no change
        return None;
    }

    // Check if BOT_HEALTH_CHECK_CHAT_ID is set in the environment
    let chat_id_env = match env::var("BOT_HEALTH_CHECK_CHAT_ID") {
        Ok(val) => val,
        Err(_) => {
            // If chat ID is not set, skip bot health check and return None (no status change)
            return None;
        }
    };

    let chat_id: i64 = match chat_id_env.parse() {
        Ok(id) => id,
        Err(_) => {
            log::error!("Invalid BOT_HEALTH_CHECK_CHAT_ID value: {}", chat_id_env);
            return Some(false); // Return false to indicate bot health check failure
        }
    };

    // Generate a time message
    let timecode =format!("Health Check\\-`{}`", now!().format("%Y%m%d%H%M%S").to_string());

    // Try sending a message with the timecode and retrieving its details
    match bot.send_message(ChatId(chat_id), &timecode).parse_mode(ParseMode::MarkdownV2).await {
        Ok(sent_msg) => {
            // Log the success and assume the bot is healthy
            log::debug!(
                "Bot health check message sent to chat ID ({}): Message ID: ({}), Timecode: {}",
                chat_id,
                sent_msg.id,
                timecode
            );
            
            let mut sent_msgs_queue = app_state.bot_health_check_msgs.lock().await;

            sent_msgs_queue.push_back(sent_msg.id);
            if sent_msgs_queue.len() >= 10 {
                return match sent_msgs_queue.pop_front() {
                    None => {
                        log::error!("Error retrieving old messages!");
                        Some(false)
                    }
                    Some(old_msg) => {
                        match bot.delete_message(ChatId(chat_id), old_msg).await {
                            Ok(_) => Some(true),
                            Err(e) => {
                                log::error!("Failed to delete old bot health check message: {}", e);
                                Some(false)
                            }
                        }
                    }
                }
            }
            Some(true) // Bot is healthy if message sent successfully
        }
        Err(e) => {
            log::error!("Failed to send bot health check message: {}", e);
            Some(false) // Bot is unhealthy due to message send failure
        }
    }
}
