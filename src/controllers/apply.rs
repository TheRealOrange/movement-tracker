use sqlx::PgPool;
use crate::types::{RoleType, UsrType};

pub(crate) async fn apply_user(
    conn: &PgPool,
    tele_id: u64,
    name: String,
    ops_name: String,
    role_type: RoleType,
    user_type: UsrType
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        INSERT INTO apply (tele_id, name, ops_name, role_type, usr_type)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (tele_id) DO NOTHING;
        "#,
        tele_id as i64,   // Assuming `tele_id` is INT8 (bigint), so we cast `u64` to `i64`.
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