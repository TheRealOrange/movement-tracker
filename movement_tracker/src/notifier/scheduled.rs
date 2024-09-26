use std::sync::Arc;
use crate::types::{Availability, ScheduledNotifications, Usr, UsrType};
use sqlx::PgPool;
use std::time::Duration;
use teloxide::prelude::*;
use crate::{utils, AppState};

pub(crate) async fn start_notifier(bot: Bot, state: Arc<AppState>) -> Result<(), sqlx::Error> {
    loop {
        // Wait for a specified duration (e.g., 60 seconds)
        tokio::time::sleep(Duration::from_secs(60)).await;

        // Attempt to Process Scheduled Notifications
        match process_scheduled_notifications(&state.db_pool, &bot).await {
            Ok(_) => {
                // Set Notifier Status to Healthy
                let mut notifier = state.notifier_status.lock().await;
                *notifier = true;
                log::debug!("Notifier task processed notifications successfully.");
            }
            Err(e) => {
                // Set Notifier Status to Unhealthy
                let mut notifier = state.notifier_status.lock().await;
                *notifier = false;
                log::error!("Notifier task failed to process notifications: {}", e);
            }
        }
    }
}

async fn process_scheduled_notifications(conn: &PgPool, bot: &Bot) -> Result<(), sqlx::Error> {
    // Start a transaction
    let mut tx = conn.begin().await?;

    // Wrap the entire logic in a block that ensures rollback on error
    match async {
        // Fetch due notifications
        let notifications = sqlx::query_as!(
            ScheduledNotifications,
            r#"
            SELECT
                sn.id,
                sn.avail_id,
                sn.scheduled_time,
                sn.sent,
                sn.created,
                sn.updated,
                sn.is_valid
            FROM scheduled_notifications sn
            WHERE sn.scheduled_time <= NOW()
              AND sn.sent = FALSE
              AND sn.is_valid = TRUE
            FOR UPDATE SKIP LOCKED;
            "#
        )
            .fetch_all(&mut *tx)
            .await?;

        // Check if there are no notifications to process
        if notifications.is_empty() {
            log::debug!("No scheduled notifications to process.");
            return Ok::<(), sqlx::Error>(());
        }

        // Process each notification
        for notification in notifications {
            // Fetch associated availability
            log::debug!("Fetching availability with ID: {}", notification.avail_id);
            let availability = match sqlx::query_as!(
                Availability,
                r#"
                SELECT
                    a.id,
                    a.usr_id AS user_id,
                    a.avail,
                    a.ict_type AS "ict_type: _",
                    a.remarks,
                    a.planned,
                    a.saf100,
                    a.attended,
                    a.is_valid,
                    a.created,
                    a.updated
                FROM availability a
                WHERE a.id = $1
                  AND a.is_valid = TRUE;
                "#,
                notification.avail_id
            )
                .fetch_optional(&mut *tx)
                .await? {
                Some(avail) => avail,
                None => {
                    log::warn!(
                        "Availability with ID {} not found or invalid. Skipping notification ID {}.",
                        notification.avail_id, notification.id
                    );
                    continue; // Skip processing this notification
                },
            };

            // Fetch user details
            log::debug!("Fetching user with ID: {}", availability.user_id);
            let user = match sqlx::query_as!(
                Usr,
                r#"
                SELECT
                    u.id,
                    u.tele_id,
                    u.name,
                    u.ops_name,
                    u.usr_type AS "usr_type: _",
                    u.role_type AS "role_type: _",
                    u.admin,
                    u.created,
                    u.updated
                FROM usrs u
                WHERE u.id = $1
                  AND u.is_valid = TRUE;
                "#,
                availability.user_id
            )
                .fetch_optional(&mut *tx)
                .await? {
                Some(u) => u,
                None => {
                    log::warn!(
                        "User with ID {} not found or invalid. Skipping notification ID {}.",
                        availability.user_id, notification.id
                    );
                    continue; // Skip processing this notification
                },
            };

            // Prepare detailed notification message
            let message_text = format_detailed_notification(&availability, &user)
                .unwrap_or_else(|text| text);

            // Log the notification
            log::info!(
                "Sending notification to user {} for availability on {}",
                user.ops_name,
                availability.avail
            );

            let chat_id = ChatId(user.tele_id);

            // Send the formatted message to the user with MarkdownV2 parsing
            if let Err(e) = bot.send_message(chat_id, message_text)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await {
                log::error!("Error sending message to user {}: {}", user.ops_name, e);
            }

            // Mark the notification as sent
            sqlx::query!(
                r#"
                UPDATE scheduled_notifications
                SET sent = TRUE
                WHERE id = $1;
                "#,
                notification.id
            )
                .execute(&mut *tx)
                .await?;
        }

        Ok(())
    }.await {
        Ok(_) => {
            // If the block succeeds, commit the transaction
            tx.commit().await?;
            Ok(())
        }
        Err(e) => {
            // If an error occurs, rollback the transaction
            log::error!("Error processing scheduled notifications, rolling back transaction.");
            tx.rollback().await?;
            Err(e)
        } 
    }
}

/// Formats a detailed notification message with proper MarkdownV2 escaping
fn format_detailed_notification(availability: &Availability, user: &Usr) -> Result<String, String> {
    // Escape special characters to prevent Markdown parsing issues
    let date_str = availability.avail.format("%b %d, %Y").to_string();
    let ict_type_str = &availability.ict_type.as_ref();

    // Handle optional remarks with truncation and escaping
    let remarks_str = match &availability.remarks {
        Some(remarks) => {
            let truncated = if remarks.chars().count() > 50 {
                format!("{}...", remarks.chars().take(50).collect::<String>())
            } else {
                remarks.clone()
            };
            utils::escape_special_characters(&truncated)
        },
        None => "None".to_string(),
    };

    // Determine SAF100 status based on user type and flags
    let saf100_str = match user.usr_type {
        UsrType::NS => {
            if availability.saf100 {
                "*SAF100 Issued*".to_string()
            } else if availability.planned {
                "*SAF100 Pending*".to_string()
            } else {
                "".to_string()
            }
        },
        _ => "".to_string(),
    };

    // Compile the message with MarkdownV2 formatting
    let mut message = format!(
        "*Reminder: Upcoming Planned Event*\n\n\
         *Date:* {}\n\
         *Type:* {}\n\
         *Remarks:* {}\n",
        date_str, ict_type_str, remarks_str
    );

    if !saf100_str.is_empty() {
        message.push_str(&format!("{}\n", saf100_str));
    }

    // Add any additional information if necessary
    message.push_str("\nPlease wait for the flight schedule to be sent\\.");

    Ok(message)
}