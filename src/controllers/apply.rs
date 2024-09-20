use crate::types::{Apply, RoleType, UsrType};
use sqlx::types::Uuid;
use sqlx::PgPool;

pub(crate) async fn apply_user(
    conn: &PgPool,
    tele_id: u64,
    chat_username: String,
    name: String,
    ops_name: String,
    role_type: RoleType,
    user_type: UsrType
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        INSERT INTO apply (tele_id, chat_username, name, ops_name, role_type, usr_type)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (tele_id) DO NOTHING;
        "#,
        tele_id as i64,   // Assuming `tele_id` is INT8 (bigint), so we cast `u64` to `i64`.
        chat_username,
        name,
        ops_name,
        role_type as RoleType,  // Enum cast to DB-compatible type
        user_type as UsrType    // Enum cast to DB-compatible type
    )
        .execute(conn)
        .await;

    match result {
        Ok(query_result) => {
            if query_result.rows_affected() == 1 {
                log::info!("User with tele_id: ({}) and name: ({}) applied", tele_id, name);
                Ok(true)  // Application was successful
            } else {
                log::info!("User with tele_id: ({}) already applied", tele_id);
                Ok(false)  // User already applied, no new row inserted
            }
        }
        Err(e) => {
            log::error!("Error inserting user application: {}", e);
            Err(e)  // Return the SQL error
        }
    }
}

// Function to fetch all apply requests
pub async fn get_all_apply_requests(pool: &PgPool) -> Result<Vec<Apply>, sqlx::Error> {
    let result = sqlx::query_as!(
        Apply,
        r#"
        SELECT
            apply.id AS id,
            apply.tele_id AS tele_id,
            apply.chat_username AS chat_username,
            apply.name AS name,
            apply.ops_name AS ops_name,
            apply.usr_type AS "usr_type: _",
            apply.role_type AS "role_type: _",
            apply.created AS created,
            apply.updated AS updated
        FROM apply
        WHERE apply.is_valid = TRUE;  -- Fetch only valid apply requests
        "#
    )
        .fetch_all(pool)
        .await;

    match result {
        Ok(query_result) => {
            log::info!("Successfully fetched {} valid apply requests", query_result.len());
            Ok(query_result)
        }
        Err(e) => {
            log::error!("Failed to fetch valid apply requests: {:?}", e);
            Err(e)
        }
    }
}


pub(crate) async fn get_apply_by_uuid(conn: &PgPool, id: Uuid) -> Result<Apply, sqlx::Error> {
    let result = sqlx::query_as!(
        Apply,
        r#"
        SELECT
            id,
            tele_id,
            chat_username,
            name,
            ops_name,
            usr_type AS "usr_type: _",
            role_type AS "role_type: _",
            created,
            updated
        FROM apply
        WHERE id = $1 AND is_valid = TRUE;  -- Only fetch valid apply requests
        "#,
        id
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(apply) => {
            log::info!("Found valid apply request with id: {}", id);
            Ok(apply)
        }
        Err(e) => {
            log::error!("Error fetching valid apply request by id: {}", e);
            Err(e)
        }
    }
}


pub(crate) async fn remove_apply_by_uuid(conn: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE apply
        SET is_valid = FALSE  -- Soft delete by marking as invalid
        WHERE id = $1 AND is_valid = TRUE;  -- Only update if the record is valid
        "#,
        id
    )
        .execute(conn)
        .await;

    match result {
        Ok(query_result) => {
            if query_result.rows_affected() == 1 {
                log::info!("Successfully soft-deleted apply request with id: {}", id);
                Ok(true)
            } else {
                log::warn!("No valid apply request found with id: {}", id);
                Ok(false)
            }
        }
        Err(e) => {
            log::error!("Error soft-deleting apply request by id: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn apply_exists_tele_id(conn: &PgPool, tele_id: u64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM apply
            WHERE apply.tele_id = $1
            AND apply.is_valid = TRUE  -- Only check valid apply requests
        ) AS "exists!";
        "#,
        tele_id as i64
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            let exists = res.exists;
            if exists {
                log::info!("Valid apply request for tele_id: {} exists", tele_id);
            } else {
                log::info!("No valid apply request for tele_id: {} exists", tele_id);
            }
            Ok(exists)
        }
        Err(e) => {
            log::error!("Error checking if valid apply request exists for tele_id: {}: {}", tele_id, e);
            Err(e)
        }
    }
}


pub(crate) async fn get_apply_by_tele_id(conn: &PgPool, tele_id: u64) -> Result<Apply, sqlx::Error> {
    let result = sqlx::query_as!(
        Apply,
        r#"
        SELECT
            id,
            tele_id,
            chat_username,
            name,
            ops_name,
            usr_type AS "usr_type: _",
            role_type AS "role_type: _",
            created,
            updated
        FROM apply
        WHERE tele_id = $1
        AND is_valid = TRUE;  -- Only fetch valid apply requests
        "#,
        tele_id as i64
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(apply) => {
            log::info!("Successfully retrieved valid apply request with tele_id: {}", tele_id);
            Ok(apply)
        }
        Err(e) => {
            log::error!("Error fetching valid apply request by tele_id: {}: {}", tele_id, e);
            Err(e)
        }
    }
}

pub(crate) async fn user_has_pending_application(conn: &PgPool, tele_id: u64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM apply
            WHERE is_valid = TRUE
            AND tele_id = $1
        ) AS "exists!"
        "#,
        tele_id as i64
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            let exists = res.exists;

            if exists {
                log::info!("User with telegram_id: ({}) has a pending application.", tele_id);
            } else {
                log::info!("User with telegram_id: ({}) does not have a pending application.", tele_id);
            }

            Ok(exists)
        }
        Err(e) => {
            log::error!("Error checking pending application for telegram_id {}: {}", tele_id, e);
            Err(e)
        }
    }
}