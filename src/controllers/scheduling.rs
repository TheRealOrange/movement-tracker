use crate::types::{Availability, AvailabilityDetails, Ict, Usr};
use sqlx::types::chrono::{NaiveDate, Utc};
use sqlx::PgPool;
use sqlx::types::Uuid;

async fn check_user_avail(conn: &PgPool, tele_id: i64, date: NaiveDate) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM availability
            INNER JOIN usrs
            ON availability.usr_id = usrs.id
            WHERE usrs.tele_id = $1
            AND availability.avail = $2
            AND availability.is_valid = TRUE
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
pub(crate) async fn edit_avail_by_uuid(
    conn: &PgPool,
    availability_id: Uuid,
    ict_type: Option<Ict>,
    remarks: Option<String>,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH update_statement AS (
            UPDATE availability
            SET
                ict_type = COALESCE($2, availability.ict_type),
                remarks = COALESCE($3, availability.remarks),
                is_valid = TRUE  -- Ensure that the availability is revalidated
            WHERE availability.id = $1
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
        FROM usrs
        JOIN availability ON usrs.id = availability.usr_id
        JOIN update_statement ON availability.id = $1;
        "#,
        availability_id,
        ict_type as _,
        remarks
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!("Updated availability with UUID: {}", availability_id);
            Ok(res)
        }
        Err(e) => {
            log::error!("Error updating availability with UUID {}: {}", availability_id, e);
            Err(e)
        }
    }
}

pub(crate) async fn set_user_unavail(
    conn: &PgPool,
    availability_id: Uuid,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH update_statement AS (
            UPDATE availability
            SET is_valid = FALSE
            WHERE availability.id = $1
            RETURNING
            avail,
            ict_type,
            remarks,
            saf100,
            attended,
            availability.created,
            availability.updated,
            usr_id
        ) SELECT
        usrs.ops_name,
        update_statement.avail,
        update_statement.ict_type AS "ict_type: _",
        update_statement.remarks,
        update_statement.saf100,
        update_statement.attended,
        update_statement.created,
        update_statement.updated
        FROM update_statement
        JOIN usrs ON update_statement.usr_id = usrs.id;
        "#,
        availability_id
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!("Soft deleted availability with id: {}", availability_id);
            Ok(res)
        }
        Err(e) => {
            log::error!("Error soft-deleting availability by id: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn get_upcoming_availability_by_tele_id(
    conn: &PgPool,
    tele_id: u64
) -> Result<Vec<Availability>, sqlx::Error> {
    let today = Utc::now().date_naive();  // Get today's date

    let result = sqlx::query_as!(
        Availability,
        r#"
        SELECT
            availability.id,
            availability.usr_id AS user_id,
            availability.avail,
            availability.ict_type AS "ict_type: _",
            availability.remarks,
            availability.saf100,
            availability.attended,
            availability.created,
            availability.updated
        FROM availability
        JOIN usrs ON usrs.id = availability.usr_id
        WHERE usrs.tele_id = $1
        AND availability.avail >= $2
        AND availability.is_valid = TRUE  -- Only fetch valid availability
        ORDER BY availability.avail ASC;
        "#,
        tele_id as i64,
        today
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(availability_list) => {
            log::info!(
                "Found {} upcoming availability entries for tele_id: {}",
                availability_list.len(),
                tele_id
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!(
                "Error fetching upcoming availability for tele_id {}: {}",
                tele_id,
                e
            );
            Err(e)
        }
    }
}

pub(crate) async fn get_availability_by_uuid(
    conn: &PgPool,
    availability_id: Uuid,
) -> Result<Availability, sqlx::Error> {
    let result = sqlx::query_as!(
        Availability,
        r#"
        SELECT
            availability.id,
            availability.usr_id AS user_id,
            availability.avail,
            availability.ict_type AS "ict_type: _",
            availability.remarks,
            availability.saf100,
            availability.attended,
            availability.created,
            availability.updated
        FROM availability
        WHERE availability.id = $1
        AND availability.is_valid = TRUE;  -- Only fetch valid availability entries
        "#,
        availability_id
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(availability) => {
            log::info!("Found valid availability with id: {}", availability_id);
            Ok(availability)
        }
        Err(e) => {
            log::error!("Error fetching valid availability by id: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn add_user_avail(
    conn: &PgPool,
    tele_id: u64,
    date: NaiveDate,
    ict_type: &Ict,
    remarks: Option<String>,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH insert_statement AS (
            INSERT INTO availability (usr_id, avail, ict_type, remarks)
            SELECT usrs.id, $1, $2, $3
            FROM usrs
            WHERE usrs.tele_id = $4
            RETURNING
                avail,
                ict_type,
                remarks,
                saf100,
                attended,
                created,
                updated
        )
        SELECT
            usrs.ops_name,
            insert_statement.avail,
            insert_statement.ict_type AS "ict_type: _",
            insert_statement.remarks,
            insert_statement.saf100,
            insert_statement.attended,
            insert_statement.created,
            insert_statement.updated
        FROM usrs
        JOIN insert_statement ON usrs.tele_id = $4;
        "#,
        date,
        ict_type as _,
        remarks,
        tele_id as i64
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!("Added new availability for tele_id: {} on {}", tele_id, date);
            Ok(res)
        }
        Err(e) => {
            log::error!("Error inserting availability for tele_id {}: {}", tele_id, e);
            Err(e)
        }
    }
}