use chrono::Utc;

use sqlx::types::chrono::NaiveDate;
use sqlx::PgPool;
use sqlx::types::Uuid;

use crate::types::{AvailabilityDetails, UsrType};
use crate::{now, APP_TIMEZONE};

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
                availability.id,
                avail,
                ict_type,
                remarks,
                planned,
                saf100,
                attended,
                availability.is_valid,
                availability.created,
                availability.updated
        )
        SELECT
            update_statement.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            update_statement.avail,
            update_statement.ict_type AS "ict_type: _",
            update_statement.remarks,
            update_statement.planned,
            update_statement.saf100,
            update_statement.attended,
            update_statement.is_valid,
            update_statement.created,
            update_statement.updated
        FROM usrs, update_statement
        WHERE usrs.tele_id = $2 AND usrs.is_valid = TRUE;
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

pub(crate) async fn get_future_planned_availability_for_ns(
    conn: &PgPool,
) -> Result<Vec<AvailabilityDetails>, sqlx::Error> {
    let today = now!().date_naive(); // Get today's date

    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        SELECT
            availability.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            availability.avail,
            availability.ict_type AS "ict_type: _",
            availability.remarks,
            availability.planned,
            availability.saf100,
            availability.attended,
            availability.is_valid,
            availability.created,
            availability.updated
        FROM availability
        JOIN usrs ON usrs.id = availability.usr_id
        WHERE usrs.usr_type = $1 AND usrs.is_valid = TRUE
          AND availability.planned = TRUE
          AND availability.avail >= $2
        ORDER BY availability.avail ASC;
        "#,
        UsrType::NS as _,
        today
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(availability_list) => {
            log::info!(
                "Found {} planned future availability entries for NS users",
                availability_list.len()
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!(
                "Error fetching planned future availability entries for NS users: {}",
                e
            );
            Err(e)
        }
    }
}

pub(crate) async fn get_future_valid_availability_for_ns(
    conn: &PgPool,
) -> Result<Vec<AvailabilityDetails>, sqlx::Error> {
    let today = now!().date_naive(); // Get today's date

    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        SELECT
            availability.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            availability.avail,
            availability.ict_type AS "ict_type: _",
            availability.remarks,
            availability.planned,
            availability.saf100,
            availability.attended,
            availability.is_valid,
            availability.created,
            availability.updated
        FROM availability
        JOIN usrs ON usrs.id = availability.usr_id
        WHERE usrs.usr_type = $1 AND usrs.is_valid = TRUE
          AND availability.is_valid = TRUE
          AND availability.avail >= $2
        ORDER BY availability.avail ASC;
        "#,
        UsrType::NS as _,
        today
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(availability_list) => {
            log::info!(
                "Found {} valid future availability entries for NS users",
                availability_list.len()
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!(
                "Error fetching valid future availability entries for NS users: {}",
                e
            );
            Err(e)
        }
    }
}


pub(crate) async fn set_saf100_true_by_uuid(
    conn: &PgPool,
    id: Uuid,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH update_statement AS (
            UPDATE availability
            SET saf100 = TRUE,
                updated = NOW()
            FROM usrs
            WHERE availability.id = $1
              AND availability.is_valid = TRUE  -- Only update valid entries
              AND availability.usr_id = usrs.id
            RETURNING
                availability.id,
                availability.avail,
                availability.ict_type,
                availability.remarks,
                availability.planned,
                availability.saf100,
                availability.attended,
                availability.is_valid,
                availability.created,
                availability.updated
        )
        SELECT
            update_statement.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            update_statement.avail,
            update_statement.ict_type AS "ict_type: _",
            update_statement.remarks,
            update_statement.planned,
            update_statement.saf100,
            update_statement.attended,
            update_statement.is_valid,
            update_statement.created,
            update_statement.updated
        FROM usrs, update_statement
        WHERE usrs.id = (
            SELECT usr_id FROM availability WHERE id = $1
        ) AND usrs.is_valid = TRUE;
        "#,
        id,
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!(
                "Set SAF100 to TRUE for user '{}' on date '{}', Availability ID: {}",
                res.ops_name,
                res.avail,
                res.id
            );
            Ok(res)
        }
        Err(e) => {
            log::error!(
                "Error setting SAF100 to TRUE for Availability ID {}: {}",
                id,
                e
            );
            Err(e)
        }
    }
}