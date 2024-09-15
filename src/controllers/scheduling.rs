use crate::types::{Availability, AvailabilityDetails, Ict, Usr};
use sqlx::types::chrono::NaiveDate;
use sqlx::PgPool;

async fn check_user_avail(conn: &PgPool, tele_id: i64, date: NaiveDate) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM availability
            INNER JOIN usrs
            ON availability.usr_id = usrs.id
            WHERE usrs.tele_id = $1 AND availability.avail = $2
        );
        "#,
        tele_id,
        date
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            let exists: bool = res.exists.unwrap_or(false);

            if exists {
                log::info!(
                    "User with telegram id: ({}) is available on: ({})",
                    tele_id,
                    date
                );
            } else {
                log::info!(
                    "User with telegram id: ({}) is not available on: ({})",
                    tele_id,
                    date
                );
            }

            Ok(exists)
        }
        Err(e) => {
            log::error!("Error querying user availability: {}", e);

            Err(e)
        }
    }
}
async fn set_user_avail(
    conn: &PgPool,
    tele_id: i64,
    date: NaiveDate,
    ict_type: Option<Ict>,
    remarks: Option<&str>,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH insert_statement AS (
            INSERT INTO availability (usr_id, avail, ict_type, remarks)
            (SELECT usrs.id, $1, $2, $3
            FROM usrs
            WHERE usrs.tele_id = $4)
            ON CONFLICT (usr_id, avail) DO UPDATE
                SET
                avail = EXCLUDED.avail,
                ict_type = COALESCE(EXCLUDED.ict_type, availability.ict_type),
                remarks = COALESCE(EXCLUDED.remarks, availability.remarks)
            RETURNING
            avail,
            ict_type,
            remarks,
            saf100,
            attended,
            created,
            updated
        ) SELECT
        usrs.ops_name,
        insert_statement.avail,
        insert_statement.ict_type AS "ict_type: _",
        insert_statement.remarks,
        insert_statement.saf100,
        insert_statement.attended,
        insert_statement.created,
        insert_statement.updated
        FROM usrs, insert_statement
        WHERE usrs.tele_id = $4;
        "#,
        date,
        ict_type as _,
        remarks,
        tele_id,
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            log::info!("Set availability for ({}) on: ({})", res.ops_name, date);

            Ok(res)
        }
        Err(e) => {
            log::error!("Error inserting user availability: {}", e);

            Err(e)
        }
    }
}

async fn set_user_unavail(
    conn: &PgPool,
    tele_id: i64,
    date: NaiveDate,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH delete_statement AS (
            DELETE FROM availability
            USING usrs
            WHERE avail = $1 AND usr_id = usrs.id
            AND usrs.tele_id = $2
            RETURNING
            avail,
            ict_type,
            remarks,
            saf100,
            attended,
            availability.created,
            availability.updated
        ) SELECT
        usrs.ops_name,
        delete_statement.avail,
        delete_statement.ict_type AS "ict_type: _",
        delete_statement.remarks,
        delete_statement.saf100,
        delete_statement.attended,
        delete_statement.created,
        delete_statement.updated
        FROM usrs, delete_statement
        WHERE usrs.tele_id = $2;
        "#,
        date,
        tele_id,
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            log::info!("Removed availability for ({}) on: ({})", res.ops_name, date);

            Ok(res)
        }
        Err(e) => {
            log::error!("Error inserting user availability: {}", e);

            Err(e)
        }
    }
}
