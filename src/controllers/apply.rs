use sqlx::{query_as, PgPool};
use sqlx::types::Uuid;
use crate::types::{Apply, RoleType, UsrType};

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
    // Query to select all rows from the apply table
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
        "#
    )
        .fetch_all(pool)
        .await;

    match result {
        Ok(query_result) => {
            // Log a message when successful
            log::info!("Successfully fetched {} apply requests", query_result.len());
            Ok(query_result)
        }
        Err(e) => {
            // Log the error and return it
            log::error!("Failed to fetch apply requests: {:?}", e);
            Err(e)  // Return the SQL error
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
        WHERE id = $1;
        "#,
        id
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(apply) => {
            log::info!("Found apply request with id: {}", id);
            Ok(apply)
        }
        Err(e) => {
            log::error!("Error fetching apply request by id: {}", e);
            Err(e)
        }
    }
}


pub(crate) async fn remove_apply_by_uuid(conn: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE FROM apply
        WHERE id = $1;
        "#,
        id
    )
        .execute(conn)
        .await;

    match result {
        Ok(query_result) => {
            if query_result.rows_affected() == 1 {
                log::info!("Successfully deleted apply request with id: {}", id);
                Ok(true)  // Deletion was successful
            } else {
                log::warn!("No apply request found with id: {}", id);
                Ok(false)  // No row was deleted, indicating the UUID was not found
            }
        }
        Err(e) => {
            log::error!("Error deleting apply request by id: {}", e);
            Err(e)  // Return the SQL error
        }
    }
}