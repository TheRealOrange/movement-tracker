use std::sync::Arc;
use std::time::Duration;
use sqlx::PgPool;
use crate::AppState;

pub(crate) async fn start_audit_task(state: Arc<AppState>) -> Result<(), sqlx::Error> {
    loop {
        // Wait for 60 minutes (600 seconds)
        tokio::time::sleep(Duration::from_secs(3600)).await;

        log::info!("Starting audit task...");

        // **Attempt to Perform Audit**
        match audit_data(&state.db_pool).await {
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

async fn audit_data(conn: &PgPool) -> Result<(), sqlx::Error> {
    // Find notifications with invalid availability or users
    let problematic_notifications = sqlx::query!(
        r#"
        SELECT
            sn.id
        FROM
            scheduled_notifications sn
        LEFT JOIN
            availability a ON sn.avail_id = a.id AND a.is_valid = TRUE
        LEFT JOIN
            usrs u ON a.usr_id = u.id AND u.is_valid = TRUE
        WHERE
            sn.is_valid = TRUE
            AND a.id IS NULL
            OR u.id IS NULL;
        "#
    )
        .fetch_all(conn)
        .await?;

    for record in problematic_notifications {
        // Decide on action: log, mark as invalid, notify admins, etc.
        log::warn!("Notification ID {} has invalid or missing availability/user.", record.id);
        // Example: Mark as invalid
        sqlx::query!(
            r#"
            UPDATE scheduled_notifications
            SET is_valid = FALSE, updated = NOW()
            WHERE id = $1;
            "#,
            record.id
        )
            .execute(conn)
            .await?;
    }

    Ok(())
}