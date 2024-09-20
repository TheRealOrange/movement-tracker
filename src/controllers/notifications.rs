use sqlx::PgPool;
use crate::types::NotificationSettings;

pub(crate) async fn get_notification_settings(
    conn: &PgPool,
    chat_id: i64,
) -> Result<Option<NotificationSettings>, sqlx::Error> {
    let result = sqlx::query_as!(
        NotificationSettings,
        r#"
        SELECT
            id,
            chat_id,
            notif_system,
            notif_register,
            notif_availability,
            notif_plan,
            notif_conflict,
            created,
            updated,
            is_valid
        FROM notification_settings
        WHERE chat_id = $1 AND is_valid = TRUE;
        "#,
        chat_id
    )
        .fetch_optional(conn)
        .await;

    match result {
        Ok(option) => {
            if let Some(settings) = option {
                log::info!("Retrieved notification settings for chat_id: {}", chat_id);
                Ok(Some(settings))
            } else {
                log::info!("No notification settings found for chat_id: {}", chat_id);
                Ok(None)
            }
        }
        Err(e) => {
            log::error!("Error retrieving notification settings for chat_id {}: {}", chat_id, e);
            Err(e)
        }
    }
}

pub(crate) async fn update_notification_settings(
    conn: &PgPool,
    chat_id: i64,
    notif_system: Option<bool>,
    notif_register: Option<bool>,
    notif_availability: Option<bool>,
    notif_plan: Option<bool>,
    notif_conflict: Option<bool>,
) -> Result<NotificationSettings, sqlx::Error> {
    let result = sqlx::query_as!(
        NotificationSettings,
        r#"
        INSERT INTO notification_settings (
            chat_id,
            notif_system,
            notif_register,
            notif_availability,
            notif_plan,
            notif_conflict
        )
        VALUES ($1, COALESCE($2, FALSE), COALESCE($3, FALSE), COALESCE($4, FALSE), COALESCE($5, FALSE), COALESCE($6, FALSE))
        ON CONFLICT (chat_id) DO UPDATE SET
            notif_system = COALESCE($2, notification_settings.notif_system),
            notif_register = COALESCE($3, notification_settings.notif_register),
            notif_availability = COALESCE($4, notification_settings.notif_availability),
            notif_plan = COALESCE($5, notification_settings.notif_plan),
            notif_conflict = COALESCE($6, notification_settings.notif_conflict),
            updated = NOW()
        RETURNING
            id,
            chat_id,
            notif_system,
            notif_register,
            notif_availability,
            notif_plan,
            notif_conflict,
            created,
            updated,
            is_valid;
        "#,
        chat_id,
        notif_system,
        notif_register,
        notif_availability,
        notif_plan,
        notif_conflict
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(settings) => {
            log::info!("Updated notification settings for chat_id: {}", chat_id);
            Ok(settings)
        }
        Err(e) => {
            log::error!("Error updating notification settings for chat_id {}: {}", chat_id, e);
            Err(e)
        }
    }
}

pub(crate) async fn get_system_notifications_enabled(conn: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    // Execute the query to fetch all chat_id values where notifications are enabled
    let chat_ids = sqlx::query_scalar!(
        r#"
        SELECT
            chat_id
        FROM notification_settings
        WHERE notif_system = TRUE AND is_valid = TRUE;
        "#
    )
        .fetch_all(conn)
        .await;

    // Handle the result of the query
    match chat_ids {
        Ok(ids) => {
            if !ids.is_empty() {
                log::info!("Retrieved chat_ids with system notifications enabled: {:?} IDs retrieved", ids.len());
            } else {
                log::info!("No chats with system notifications were retrieved.");
            }
            // Return the vector of chat_ids
            Ok(ids)
        }
        Err(e) => {
            // Log the error without referencing chat_id
            log::error!("Error retrieving notification settings: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn get_register_notifications_enabled(conn: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    // Execute the query to fetch all chat_id values where notifications are enabled
    let chat_ids = sqlx::query_scalar!(
        r#"
        SELECT
            chat_id
        FROM notification_settings
        WHERE notif_register = TRUE AND is_valid = TRUE;
        "#
    )
        .fetch_all(conn)
        .await;

    // Handle the result of the query
    match chat_ids {
        Ok(ids) => {
            if !ids.is_empty() {
                log::info!("Retrieved chat_ids with register notifications enabled: {:?} IDs retrieved", ids.len());
            } else {
                log::info!("No chats with register notifications were retrieved.");
            }
            // Return the vector of chat_ids
            Ok(ids)
        }
        Err(e) => {
            // Log the error without referencing chat_id
            log::error!("Error retrieving notification settings: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn get_availability_notifications_enabled(conn: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    // Execute the query to fetch all chat_id values where notifications are enabled
    let chat_ids = sqlx::query_scalar!(
        r#"
        SELECT
            chat_id
        FROM notification_settings
        WHERE notif_availability = TRUE AND is_valid = TRUE;
        "#
    )
        .fetch_all(conn)
        .await;

    // Handle the result of the query
    match chat_ids {
        Ok(ids) => {
            if !ids.is_empty() {
                log::info!("Retrieved chat_ids with availability notifications enabled: {:?} IDs retrieved", ids.len());
            } else {
                log::info!("No chats with availability notifications were retrieved.");
            }
            // Return the vector of chat_ids
            Ok(ids)
        }
        Err(e) => {
            // Log the error without referencing chat_id
            log::error!("Error retrieving notification settings: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn get_plan_notifications_enabled(conn: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    // Execute the query to fetch all chat_id values where notifications are enabled
    let chat_ids = sqlx::query_scalar!(
        r#"
        SELECT
            chat_id
        FROM notification_settings
        WHERE notif_plan = TRUE AND is_valid = TRUE;
        "#
    )
        .fetch_all(conn)
        .await;

    // Handle the result of the query
    match chat_ids {
        Ok(ids) => {
            if !ids.is_empty() {
                log::info!("Retrieved chat_ids with plan notifications enabled: {:?} IDs retrieved", ids.len());
            } else {
                log::info!("No chats with plan notifications were retrieved.");
            }
            // Return the vector of chat_ids
            Ok(ids)
        }
        Err(e) => {
            // Log the error without referencing chat_id
            log::error!("Error retrieving notification settings: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn get_conflict_notifications_enabled(conn: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    // Execute the query to fetch all chat_id values where notifications are enabled
    let chat_ids = sqlx::query_scalar!(
        r#"
        SELECT
            chat_id
        FROM notification_settings
        WHERE notif_conflict = TRUE AND is_valid = TRUE;
        "#
    )
        .fetch_all(conn)
        .await;

    // Handle the result of the query
    match chat_ids {
        Ok(ids) => {
            if !ids.is_empty() {
                log::info!("Retrieved chat_ids with conflict notifications enabled: {:?} IDs retrieved", ids.len());
            } else {
                log::info!("No chats with conflict notifications were retrieved.");
            }
            // Return the vector of chat_ids
            Ok(ids)
        }
        Err(e) => {
            // Log the error without referencing chat_id
            log::error!("Error retrieving notification settings: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn soft_delete_notification_settings(
    conn: &PgPool,
    chat_id: i64,
) -> Result<(), sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE notification_settings
        SET is_valid = FALSE
        WHERE chat_id = $1 AND is_valid = TRUE;
        "#,
        chat_id
    )
        .execute(conn)
        .await;

    match result {
        Ok(res) => {
            if res.rows_affected() > 0 {
                log::info!("Soft deleted notification settings for chat_id: {}", chat_id);
            } else {
                log::info!("No active notification settings found to soft delete for chat_id: {}", chat_id);
            }
            Ok(())
        }
        Err(e) => {
            log::error!("Error soft deleting notification settings for chat_id {}: {}", chat_id, e);
            Err(e)
        }
    }
}