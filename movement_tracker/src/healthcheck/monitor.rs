use std::sync::Arc;
use serde::Serialize;
use sqlx::PgPool;
use teloxide::prelude::*;
use crate::{notifier, AppState};
use crate::healthcheck::bot::check_bot_health;

// Struct to represent the current health status
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct CurrentHealthStatus {
    pub database: String,
    pub notifier: String,
    pub audit: String,
    pub bot: String
}

impl CurrentHealthStatus {
    pub fn new() -> Self {
        CurrentHealthStatus {
            database: "ok".to_string(),
            notifier: "ok".to_string(),
            audit: "ok".to_string(),
            bot: "ok".to_string()
        }
    }
}

// Starts the health monitoring task
pub(crate) async fn start_health_monitor(state: Arc<AppState>) {
    // Initialize previous health status
    let mut previous_status = CurrentHealthStatus::new();

    loop {
        // Wait for a specified interval (e.g., 60 seconds)
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

        // Check current health status
        let current_status = check_health(&state).await;

        // Compare with previous status
        if current_status != previous_status {
            // Health status has changed
            log::info!("Health status changed: {:?} -> {:?}", previous_status, current_status);

            // Send notification with Emoticons
            if let Err(e) = send_health_notification(&state.bot, &current_status, &state.db_pool).await {
                log::error!("Failed to send health notification: {}", e);
            }

            // Update previous status
            previous_status = current_status;
        }
    }
}

// Checks the current health status
pub async fn check_health(state: &Arc<AppState>) -> CurrentHealthStatus {
    let mut status = CurrentHealthStatus::new();

    // Database Health Check
    match state.db_pool.acquire().await {
        Ok(mut conn) => {
            match sqlx::query("SELECT 1").execute(&mut *conn).await {
                Ok(_) => {
                    status.database = "ok".to_string();
                }
                Err(e) => {
                    log::error!("Database health check query failed: {}", e);
                    status.database = "error".to_string();
                }
            }
        }
        Err(e) => {
            log::error!("Failed to acquire database connection: {}", e);
            status.database = "error".to_string();
        }
    }

    // Notifier Health Check
    {
        let notifier = state.notifier_status.lock().await;
        status.notifier = if *notifier { "ok".to_string() } else { "error".to_string() };
    }

    // Audit Health Check
    {
        let audit = state.audit_status.lock().await;
        status.audit = if *audit { "ok".to_string() } else { "error".to_string() };
    }

    // Bot Health Check
    if let Some(bot_healthy) = check_bot_health(&state.bot, &state).await {
        let mut bot_health = state.bot_health.lock().await;
        *bot_health = bot_healthy;
        status.bot = if bot_healthy { "ok".to_string() } else { "error".to_string() };
    } else {
        // If bot health check is not active or skipped, leave the status untouched
        let bot_health = state.bot_health.lock().await;
        status.bot = if *bot_health { "ok".to_string() } else { "error".to_string() };
    }

    status
}


// Sends a health notification via Telegram with Emoticons
async fn send_health_notification(
    bot: &Bot,
    status: &CurrentHealthStatus,
    pool: &PgPool
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Determine Emoticons Based on Status
    let db_emoji = match status.database.as_str() {
        "ok" => "✅",
        "error" => "❌",
        _ => "❓",
    };

    let notifier_emoji = match status.notifier.as_str() {
        "ok" => "✅",
        "error" => "❌",
        _ => "❓",
    };

    let audit_emoji = match status.audit.as_str() {
        "ok" => "✅",
        "error" => "❌",
        _ => "❓",
    };

    let bot_emoji = match status.bot.as_str() {
        "ok" => "✅",
        "error" => "❌",
        _ => "❓",
    };

    // Construct the Message 
    let message = format!(
        "*Health Check Update*\n\n\
         *Database:* {} {}\n\
         *Notifier:* {} {}\n\
         *Audit:* {} {}\n\
         *Bot:* {} {}\n",
        db_emoji, status.database,
        notifier_emoji, status.notifier,
        audit_emoji, status.audit,
        bot_emoji, status.bot
    );
    
    notifier::emit::system_notifications(bot, message.as_str(), pool, 0).await;

    Ok(())
}