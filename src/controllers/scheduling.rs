use std::collections::HashMap;
use crate::types::{Availability, AvailabilityDetails, Ict, RoleType};
use chrono::Local;
use sqlx::types::chrono::NaiveDate;
use sqlx::types::Uuid;
use sqlx::PgPool;

pub(crate) async fn check_user_avail_multiple(
    conn: &PgPool,
    tele_id: u64,
    dates: Vec<NaiveDate>
) -> Result<Vec<Option<Availability>>, sqlx::Error> {
    if dates.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch all availability records for the user that match any of the input dates
    let result = sqlx::query_as!(
        Availability,
        r#"
        SELECT
            a.id,
            a.usr_id as user_id,
            a.avail,
            a.ict_type AS "ict_type: _",
            a.remarks,
            a.planned,
            a.saf100,
            a.attended,
            a.is_valid,
            a.created,
            a.updated
        FROM availability a
        INNER JOIN usrs u ON a.usr_id = u.id
        WHERE u.tele_id = $1
          AND a.avail = ANY($2)
          AND a.is_valid = TRUE
        "#,
        tele_id  as i64,
        &dates[..] // Pass the dates as a slice
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(records) => {
            // Map the fetched records by date for quick lookup
            let availability_map: HashMap<NaiveDate, Availability> = records.into_iter()
                .map(|record| (record.avail, record))
                .collect();

            // Build the result vector maintaining the order of input dates
            let result: Vec<Option<Availability>> = dates.into_iter()
                .map(|date| availability_map.get(&date).cloned())
                .collect();

            // Extract available dates from the availability_map
            let available_dates: Vec<NaiveDate> = availability_map.keys().cloned().collect();
            let available_dates_str: Vec<String> = available_dates
                .iter()
                .map(|date| date.format("%m/%d/%Y").to_string())
                .collect();

            log::info!(
                "User with Telegram ID ({}) has existing availability on: {}",
                tele_id,
                available_dates_str.join(", ")
            );

            Ok(result)
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
        WITH update_availability AS (
            UPDATE availability
            SET
                planned = COALESCE($2, planned),
                ict_type = COALESCE($3, ict_type),
                remarks = COALESCE($4, remarks)
            WHERE id = $1
            RETURNING *
        ),
        notification_handling AS (
            SELECT
                update_availability.planned AS new_planned,
                update_availability.id AS availability_id,
                update_availability.avail AS avail_date
            FROM update_availability
        ),
        invalidate_notifications AS (
            UPDATE scheduled_notifications
            SET is_valid = FALSE
            WHERE avail_id = (SELECT availability_id FROM notification_handling)
              AND sent = FALSE
              AND (SELECT new_planned FROM notification_handling) = FALSE
            RETURNING id
        ),
        schedule_notifications AS (
            INSERT INTO scheduled_notifications (avail_id, scheduled_time)
            SELECT
                (SELECT availability_id FROM notification_handling),
                times.scheduled_time
            FROM (
                -- Immediate Notification
                SELECT NOW() + INTERVAL '5 mins' AS scheduled_time
                UNION ALL
                -- 5 Days Prior Notification (only if at least 5 days remain)
                SELECT
                    (SELECT avail_date FROM notification_handling)::timestamp
                    + INTERVAL '09 hours'
                    - INTERVAL '5 days' AS scheduled_time
                WHERE (SELECT avail_date FROM notification_handling) - CURRENT_DATE >= 5
                UNION ALL
                -- 2 Days Prior Notification (only if at least 2 days remain)
                SELECT
                    (SELECT avail_date FROM notification_handling)::timestamp
                    + INTERVAL '09 hours'
                    - INTERVAL '2 days' AS scheduled_time
                WHERE (SELECT avail_date FROM notification_handling) - CURRENT_DATE >= 2
            ) AS times
            WHERE (SELECT new_planned FROM notification_handling) = TRUE
            RETURNING id
        )
        SELECT
            update_availability.id,
            usr.ops_name,
            usr.usr_type AS "usr_type: _",
            update_availability.avail,
            update_availability.ict_type AS "ict_type: _",
            update_availability.remarks,
            update_availability.planned,
            update_availability.saf100,
            update_availability.attended,
            update_availability.is_valid,
            update_availability.created,
            update_availability.updated
        FROM update_availability
        JOIN usrs AS usr ON update_availability.usr_id = usr.id;
        "#,
        availability_id,
        planned,
        ict_type as _,
        remarks,
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
        WITH update_availability AS (
            UPDATE availability
            SET is_valid = FALSE
            WHERE id = $1
            RETURNING *
        ),
        usr AS (
            SELECT id, ops_name, usr_type
            FROM usrs
            WHERE id = (SELECT usr_id FROM update_availability)
        )
        SELECT
            update_availability.id,
            usr.ops_name,
            usr.usr_type AS "usr_type: _",
            update_availability.avail,
            update_availability.ict_type AS "ict_type: _",
            update_availability.remarks,
            update_availability.planned,
            update_availability.saf100,
            update_availability.attended,
            update_availability.is_valid,
            update_availability.created,
            update_availability.updated
        FROM update_availability
        JOIN usr ON update_availability.usr_id = usr.id;
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
            log::error!("Error soft-deleting availability by id {}: {}", availability_id, e);
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
    planned: Option<bool>,
) -> Result<AvailabilityDetails, sqlx::Error> {
    let result = sqlx::query_as!(
        AvailabilityDetails,
        r#"
        WITH usr AS (
            SELECT id, ops_name, usr_type
            FROM usrs
            WHERE tele_id = $5
        ),
        upsert_availability AS (
            INSERT INTO availability (usr_id, avail, ict_type, remarks, planned)
            VALUES (
                (SELECT id FROM usr),
                $1,
                $2,
                $3,
                COALESCE($4, FALSE)
            )
            ON CONFLICT (usr_id, avail) DO UPDATE
                SET
                    ict_type = EXCLUDED.ict_type,
                    remarks = COALESCE(EXCLUDED.remarks, availability.remarks),
                    planned = COALESCE(EXCLUDED.planned, availability.planned)
            RETURNING *
        ),
        notification_handling AS (
            SELECT
                upsert_availability.planned AS new_planned,
                upsert_availability.id AS availability_id,
                upsert_availability.avail AS avail_date
            FROM upsert_availability
        ),
        invalidate_notifications AS (
            UPDATE scheduled_notifications
            SET is_valid = FALSE
            WHERE avail_id = (SELECT availability_id FROM notification_handling)
              AND sent = FALSE
              AND (SELECT new_planned FROM notification_handling) = FALSE
            RETURNING id
        ),
        schedule_notifications AS (
            INSERT INTO scheduled_notifications (avail_id, scheduled_time)
            SELECT
                (SELECT availability_id FROM notification_handling),
                times.scheduled_time
            FROM (
                -- Immediate Notification
                SELECT NOW() + INTERVAL '5 mins' AS scheduled_time
                UNION ALL
                -- 5 Days Prior Notification (only if at least 5 days remain)
                SELECT
                    (SELECT avail_date FROM notification_handling)::timestamp
                    + INTERVAL '09 hours'
                    - INTERVAL '5 days' AS scheduled_time
                WHERE (SELECT avail_date FROM notification_handling) - CURRENT_DATE >= 5
                UNION ALL
                -- 2 Days Prior Notification (only if at least 2 days remain)
                SELECT
                    (SELECT avail_date FROM notification_handling)::timestamp
                    + INTERVAL '09 hours'
                    - INTERVAL '2 days' AS scheduled_time
                WHERE (SELECT avail_date FROM notification_handling) - CURRENT_DATE >= 2
            ) AS times
            WHERE (SELECT new_planned FROM notification_handling) = TRUE
            RETURNING id
        )
        SELECT
            upsert_availability.id,
            usr.ops_name,
            usr.usr_type AS "usr_type: _",
            upsert_availability.avail,
            upsert_availability.ict_type AS "ict_type: _",
            upsert_availability.remarks,
            upsert_availability.planned,
            upsert_availability.saf100,
            upsert_availability.attended,
            upsert_availability.is_valid,
            upsert_availability.created,
            upsert_availability.updated
        FROM upsert_availability
        JOIN usr ON upsert_availability.usr_id = usr.id;
        "#,
        date,
        ict_type as _,
        remarks,
        planned,
        tele_id as i64,
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            log::info!(
                "Added or updated availability for tele_id: {} on {}",
                tele_id,
                date
            );
            Ok(res)
        }
        Err(e) => {
            log::error!(
                "Error inserting or updating availability for tele_id {}: {}",
                tele_id,
                e
            );
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


pub(crate) async fn get_users_available_by_role_on_date(
    conn: &PgPool,
    date: NaiveDate,
    role_type: &RoleType,
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
          AND usrs.role_type = $2
          AND (availability.is_valid = TRUE OR availability.planned = TRUE)
        ORDER BY usrs.ops_name ASC;
        "#,
        date,
        role_type as _  // Map RoleType enum
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(availability_list) => {
            log::info!(
                "Found {} users with role {:?} available on {}",
                availability_list.len(),
                role_type,
                date
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!(
                "Error fetching users with role {:?} available on {}: {}",
                role_type,
                date,
                e
            );
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
            RETURNING *
        ),
        notification_handling AS (
            SELECT
                update_statement.planned AS new_planned,
                update_statement.id AS availability_id,
                update_statement.avail AS avail_date
            FROM update_statement
        ),
        invalidate_notifications AS (
            UPDATE scheduled_notifications
            SET is_valid = FALSE
            WHERE avail_id = (SELECT availability_id FROM notification_handling)
              AND sent = FALSE
              AND (SELECT new_planned FROM notification_handling) = FALSE
            RETURNING id
        ),
        schedule_notifications AS (
            INSERT INTO scheduled_notifications (avail_id, scheduled_time)
            SELECT
                (SELECT availability_id FROM notification_handling),
                times.scheduled_time
            FROM (
                -- Immediate Notification
                SELECT NOW() + INTERVAL '5 mins' AS scheduled_time
                UNION ALL
                -- 5 Days Prior Notification (only if at least 5 days remain)
                SELECT
                    (SELECT avail_date FROM notification_handling)::timestamp
                    + INTERVAL '09 hours'
                    - INTERVAL '5 days' AS scheduled_time
                WHERE (SELECT avail_date FROM notification_handling) - CURRENT_DATE >= 5
                UNION ALL
                -- 2 Days Prior Notification (only if at least 2 days remain)
                SELECT
                    (SELECT avail_date FROM notification_handling)::timestamp
                    + INTERVAL '09 hours'
                    - INTERVAL '2 days' AS scheduled_time
                WHERE (SELECT avail_date FROM notification_handling) - CURRENT_DATE >= 2
            ) AS times
            WHERE (SELECT new_planned FROM notification_handling) = TRUE
            RETURNING id
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
        FROM update_statement
        JOIN usrs ON update_statement.usr_id = usrs.id;
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

pub(crate) async fn get_planned_availability_details_by_tele_id(
    conn: &PgPool,
    tele_id: u64,
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
        WHERE usrs.tele_id = $1
          AND availability.planned = TRUE
        ORDER BY availability.avail ASC;
        "#,
        tele_id as i64
    )
        .fetch_all(conn)
        .await;

    match result {
        Ok(availability_list) => {
            log::info!(
                "Found {} planned availability entries for tele_id: {}",
                availability_list.len(),
                tele_id
            );
            Ok(availability_list)
        }
        Err(e) => {
            log::error!(
                "Error fetching planned availability details for tele_id {}: {}",
                tele_id,
                e
            );
            Err(e)
        }
    }
}