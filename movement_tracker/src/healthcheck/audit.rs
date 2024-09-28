use std::sync::Arc;
use std::time::Duration;
use sqlx::{Error, PgPool};
use sqlx::postgres::PgQueryResult;
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

    Ok(())
}