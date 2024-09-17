use crate::types::{AvailabilityDetails, Usr};
use sqlx::types::chrono::NaiveDate;
use sqlx::PgPool;

async fn set_attendance(
    conn: &PgPool,
    tele_id: i64,
    date: NaiveDate,
    attended: bool,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH update_statement AS (
            UPDATE availability
            SET attended = $1
            FROM usrs
            WHERE usr_id = usrs.id 
            AND usrs.tele_id = $2
            AND availability.avail = $3
            AND availability.is_valid = TRUE  -- Only update valid entries
            RETURNING
                avail,
                ict_type,
                remarks,
                saf100,
                attended,
                availability.created,
                availability.updated
        )
        SELECT
            usrs.ops_name,
            update_statement.avail,
            update_statement.ict_type AS "ict_type: _",
            update_statement.remarks,
            update_statement.saf100,
            update_statement.attended,
            update_statement.created,
            update_statement.updated
        FROM usrs, update_statement
        WHERE usrs.tele_id = $2;
        "#,
        attended,
        tele_id,
        date,
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!(
                "Attendance status ({}) updated for user ({}) on: ({})",
                res.attended, res.ops_name, date
            );
            Ok(res)
        }
        Err(e) => {
            log::error!("Error updating user attendance: {}", e);
            Err(e)
        }
    }
}
