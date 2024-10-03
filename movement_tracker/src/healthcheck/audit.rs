use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::Requester;
use teloxide::types::ChatId;
use crate::{controllers, AppState};

pub(crate) async fn start_audit_task(state: Arc<AppState>) -> Result<(), sqlx::Error> {
    loop {
        // Wait for 60 minutes (600 seconds)
        tokio::time::sleep(Duration::from_secs(3600)).await;

        log::info!("Starting audit task...");

        // **Attempt to Perform Audit**
        match audit_data(&state).await {
            Ok(_) => {
                // **Set Audit Status to Healthy**
                let mut audit = state.audit_status.lock().await;
                *audit = true;
                log::info!("Audit task completed successfully.");
            }
            Err(e) => {
                // **Set Audit Status to Unhealthy**
                let mut audit = state.audit_status.lock().await;
                *audit = false;
                log::error!("Audit task failed: {}", e);
            }
        }
    }
}

async fn check_remove_chat(chat_id: ChatId, error: bool, state: &AppState) -> Result<(), sqlx::Error> {
    let mut errored_set = state.notification_remove_list.lock().await;
    if !error {
        *errored_set.entry(chat_id).or_insert(0) = 0;
    } else {
        *errored_set.entry(chat_id).or_insert(0) += 1;
    }

    if errored_set[&chat_id] > 2 {
        controllers::notifications::soft_delete_notification_settings(&state.db_pool, chat_id.0).await?;
    }

    Ok(())
}

async fn audit_data(state: &AppState) -> Result<(), sqlx::Error> {
    let conn = &state.db_pool;
    // Find notifications with invalid availability or users
    let problematic_notifications = sqlx::query!(
        r#"
        SELECT
            sn.id
        FROM
            scheduled_notifications sn
        LEFT JOIN
            availability a ON sn.avail_id = a.id
        LEFT JOIN
            usrs u ON a.usr_id = u.id
        WHERE
            sn.is_valid = TRUE
            AND (a.is_valid = FALSE OR u.is_valid = FALSE);
        "#
    )
        .fetch_all(conn)
        .await?;

    for record in problematic_notifications {
        // log, mark as invalid
        log::warn!("Notification ID {} has invalid or missing availability/user, invalidating.", record.id);
        // Mark as invalid
        let result = sqlx::query!(
            r#"
            UPDATE scheduled_notifications
            SET is_valid = FALSE
            WHERE id = $1;
            "#,
            record.id
        )
            .execute(conn)
            .await;
        
        match result {
            Ok(_) => {
                log::debug!("Notification ID {} invalidated", record.id);
            }
            Err(e) => {
                log::error!("Failed to mark notification ID {} as invalid: {:?}", record.id, e);
            }
        }
    }

    // Now, find all the chats for which notifications have been configured
    // and check if they are still valid by calling get chat member
    let bot = &state.bot;
    let my_id = match bot.get_me().await {
        Ok(me) => me.id,
        Err(e) => {
            // Bot API seems to not be working, abort the notification validity
            // checking for now
            log::error!("Failed to get own ID: {:?}", e);
            return Ok(());
        }
    };

    match controllers::notifications::get_notifications_enabled(conn).await {
        Ok(chat_ids) => {
            for chat_id in chat_ids {
                let success: bool;
                match bot.get_chat_member(ChatId(chat_id), my_id).await {
                    Ok(chat_member) => {
                        // Success, bot is still a member of the chat
                        if chat_member.user.id != my_id {
                            // Somehow doesn't match? add to check deletion
                            log::error!("Failed to match chat member for chat ID ({}): {:?}", chat_id, chat_member);
                            success = false;
                        } else {
                            success = true;
                        }
                    }
                    Err(e) => {
                        // Unable to get bot as member of chat, bot may have been removed from chat
                        log::error!("Failed to get chat member for chat ID {}: {:?}", chat_id, e);
                        success = false;
                    }
                }

                check_remove_chat(ChatId(chat_id), success, state).await?;
            }
        }
        Err(e) => {
            log::error!("Failed to check enabled notifications: {:?}", e);
        }
    }

    Ok(())
}