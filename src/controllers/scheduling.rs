use chrono::Local;
use crate::types::{Availability, AvailabilityDetails, Ict, RoleType, Usr};
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
            AND (availability.is_valid = TRUE OR availability.planned = TRUE)
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
    planned: Option<bool>,
    ict_type: Option<Ict>,
    remarks: Option<String>,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH update_statement AS (
            UPDATE availability
            SET
                planned = COALESCE($2, availability.planned),
                ict_type = COALESCE($3, availability.ict_type),
                remarks = COALESCE($4, availability.remarks),
                is_valid = TRUE  -- Ensure that the availability is revalidated
            WHERE availability.id = $1
            RETURNING
                availability.id,
                avail,
                planned,
                ict_type,
                remarks,
                saf100,
                attended,
                is_valid,
                availability.created,
                availability.updated
        )
        SELECT
            update_statement.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            update_statement.avail,
            update_statement.planned,
            update_statement.ict_type AS "ict_type: _",
            update_statement.remarks,
            update_statement.saf100,
            update_statement.attended,
            update_statement.is_valid,
            update_statement.created,
            update_statement.updated
        FROM usrs
        JOIN availability ON usrs.id = availability.usr_id
        JOIN update_statement ON availability.id = $1;
        "#,
        availability_id,
        planned,
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
                id,
                avail,
                planned,
                ict_type,
                remarks,
                saf100,
                attended,
                is_valid,
                availability.created,
                availability.updated,
            usr_id
        ) SELECT
            update_statement.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            update_statement.avail,
            update_statement.planned,
            update_statement.ict_type AS "ict_type: _",
            update_statement.remarks,
            update_statement.saf100,
            update_statement.attended,
            update_statement.is_valid,
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
    let today = Local::now().date_naive();  // Get today's date

    let result = sqlx::query_as!(
        Availability,
        r#"
        SELECT
            availability.id,
            availability.usr_id AS user_id,
            availability.avail,
            availability.planned,
            availability.ict_type AS "ict_type: _",
            availability.remarks,
            availability.saf100,
            availability.attended,
            availability.is_valid,
            availability.created,
            availability.updated
        FROM availability
        JOIN usrs ON usrs.id = availability.usr_id
        WHERE usrs.tele_id = $1
        AND availability.avail >= $2
        AND (availability.is_valid = TRUE OR availability.planned = TRUE)  -- Only fetch valid availability
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

pub(crate) async fn get_upcoming_availability_details_by_tele_id(
    conn: &PgPool,
    tele_id: u64,
) -> Result<Vec<AvailabilityDetails>, sqlx::Error> {
    let today = Local::now().date_naive(); // Get today's date

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
        WHERE usrs.tele_id = $1
          AND availability.avail >= $2
          AND (availability.is_valid = TRUE OR availability.planned = TRUE)
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
                "Found {} upcoming availability entries with details for tele_id: {}",
                availability_list.len(),
                tele_id
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!(
                "Error fetching upcoming availability details for tele_id {}: {}",
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
            availability.planned,
            availability.ict_type AS "ict_type: _",
            availability.remarks,
            availability.saf100,
            availability.attended,
            availability.is_valid,
            availability.created,
            availability.updated
        FROM availability
        WHERE availability.id = $1
        AND (availability.is_valid = TRUE OR availability.planned = TRUE);  -- Only fetch valid availability entries
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
    planned: Option<bool>
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH insert_statement AS (
            INSERT INTO availability (usr_id, avail, ict_type, remarks, planned)
            SELECT usrs.id, $1, $2, $3, COALESCE($5, FALSE)  -- Use COALESCE to set planned to FALSE if None
            FROM usrs
            WHERE usrs.tele_id = $4
            ON CONFLICT (usr_id, avail) DO UPDATE
                SET
                    ict_type = EXCLUDED.ict_type,
                    remarks = COALESCE(EXCLUDED.remarks, availability.remarks),
                    planned = COALESCE(EXCLUDED.planned, availability.planned),
                    is_valid = TRUE -- Revalidate the availability
            RETURNING
                id,
                usr_id,
                avail,
                ict_type,
                remarks,
                planned,  -- Return the planned field as well
                saf100,
                attended,
                is_valid,
                created,
                updated
        )
        SELECT
            insert_statement.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            insert_statement.avail,
            insert_statement.ict_type AS "ict_type: _",
            insert_statement.remarks,
            insert_statement.planned,  -- Include planned in the SELECT part
            insert_statement.saf100,
            insert_statement.attended,
            insert_statement.is_valid,
            insert_statement.created,
            insert_statement.updated
        FROM usrs
        JOIN insert_statement ON usrs.id = insert_statement.usr_id
        WHERE usrs.tele_id = $4;
        "#,
        date,
        ict_type as _,
        remarks,
        tele_id as i64,
        planned  // Pass the planned parameter to the query
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!("Added or updated availability for tele_id: {} on {}", tele_id, date);
            Ok(res)
        }
        Err(e) => {
            log::error!("Error inserting or updating availability for tele_id {}: {}", tele_id, e);
            Err(e)
        }
    }
}

pub(crate) async fn get_availability_for_role_and_dates(
    conn: &PgPool,
    role_type: RoleType,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<AvailabilityDetails>, sqlx::Error> {
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
        WHERE usrs.role_type = $1
        AND availability.avail >= $2
        AND availability.avail <= $3
        AND (availability.is_valid = TRUE OR availability.planned = TRUE)
        ORDER BY availability.avail ASC;
        "#,
        role_type as _,  // RoleType enum
        start,           // Start date
        end              // End date
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(availability_list) => {
            log::info!(
                "Found {} availability entries for role: {:?} from {} to {}",
                availability_list.len(),
                role_type,
                start,
                end
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!(
                "Error fetching availability for role {:?} between {} and {}: {}",
                role_type,
                start,
                end,
                e
            );
            Err(e)
        }
    }
}

pub(crate) async fn get_furthest_avail_date_for_role(
    conn: &PgPool,
    role_type: &RoleType,
) -> Result<Option<NaiveDate>, sqlx::Error> {
    let result = sqlx::query_scalar!(
        r#"
        SELECT MAX(availability.avail)
        FROM availability
        JOIN usrs ON usrs.id = availability.usr_id
        WHERE usrs.role_type = $1
        AND (availability.is_valid = TRUE OR availability.planned = TRUE);
        "#,
        role_type as _  // RoleType enum
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(date) => {
            log::info!(
                "Furthest availability date for role {:?}: {}",
                role_type,
                date.map_or("None".to_string(), |d| d.to_string())
            );
            Ok(date)
        }
        Err(e) => {
            log::error!(
                "Error retrieving furthest availability date for role {:?}: {}",
                role_type,
                e
            );
            Err(e)
        }
    }
}

pub(crate) async fn get_users_available_on_date(
    conn: &PgPool,
    date: NaiveDate,
) -> Result<Vec<AvailabilityDetails>, sqlx::Error> {
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
        WHERE availability.avail = $1
        AND (availability.is_valid = TRUE OR availability.planned = TRUE)
        ORDER BY usrs.ops_name ASC;
        "#,
        date
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(availability_list) => {
            log::info!(
                "Found {} users available on {}",
                availability_list.len(),
                date
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!("Error fetching users available on {}: {}", date, e);
            Err(e)
        }
    }
}

pub(crate) async fn toggle_planned_status(
    conn: &PgPool,
    availability_id: Uuid,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH update_statement AS (
            UPDATE availability
            SET planned = NOT planned
            WHERE id = $1
            RETURNING
                id,
                usr_id,
                avail,
                ict_type,
                remarks,
                planned,
                saf100,
                attended,
                is_valid,
                created,
                updated
        )
        SELECT
            update_statement.id,
            usrs.ops_name,
            usrs.usr_type AS "usr_type: _",
            update_statement.avail,
            update_statement.planned,
            update_statement.ict_type AS "ict_type: _",
            update_statement.remarks,
            update_statement.saf100,
            update_statement.attended,
            update_statement.is_valid,
            update_statement.created,
            update_statement.updated
        FROM usrs
        JOIN update_statement ON usrs.id = update_statement.usr_id;
        "#,
        availability_id
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!(
                "Toggled planned status for availability with UUID: {}. New planned status: {}",
                availability_id,
                res.planned
            );
            Ok(res)
        }
        Err(e) => {
            log::error!(
                "Error toggling planned status for availability with UUID {}: {}",
                availability_id,
                e
            );
            Err(e)
        }
    }
}