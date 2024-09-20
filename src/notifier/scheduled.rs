use crate::types::{Availability, ScheduledNotifications, Usr};
use sqlx::PgPool;
use std::time::Duration;
use teloxide::prelude::*;

pub(crate) async fn start_notifier(bot: Bot, conn: PgPool) {
    loop {
        // Wait for a specified duration (e.g., 60 seconds)
        tokio::time::sleep(Duration::from_secs(60)).await;

        // Process scheduled notifications
        if let Err(e) = process_scheduled_notifications(&conn, &bot).await {
            log::error!("Error processing scheduled notifications: {}", e);
        }
    }
}

pub(crate) async fn process_scheduled_notifications(conn: &PgPool, bot: &Bot) -> Result<(), sqlx::Error> {
    // Start a transaction
    let mut tx = conn.begin().await?;

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

    for notification in notifications {
        // Fetch associated availability
        let availability = sqlx::query_as!(
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
            .fetch_one(&mut *tx)
            .await?;

        // Fetch user details
        let user = sqlx::query_as!(
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
            .fetch_one(&mut *tx)
            .await?;

        // Send notification to the user
        log::info!(
            "Sending notification to user {} for availability on {}",
            user.ops_name, availability.avail
        );

        let chat_id = ChatId(user.tele_id);

        let message_text = format!(
            "Reminder: You have a planned availability on {}.",
            availability.avail
        );

        if let Err(e) = bot.send_message(chat_id, message_text).await {
            log::error!("Error sending message to user {}: {}", user.ops_name, e);
        }

        // Mark the notification as sent
        sqlx::query!(
            r#"
            UPDATE scheduled_notifications
            SET sent = TRUE, updated = NOW()
            WHERE id = $1;
            "#,
            notification.id
        )
            .execute(&mut *tx)
            .await?;
    }

    // Commit the transaction
    tx.commit().await?;

    Ok(())
}