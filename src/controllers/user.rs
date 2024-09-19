use crate::types::{RoleType, Usr, UsrType};
use sqlx::types::Uuid;
use sqlx::PgPool;

pub(crate) async fn user_exists_tele_id(conn: &PgPool, tele_id: u64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM usrs
            WHERE usrs.tele_id = $1 AND is_valid = TRUE
        ) AS "exists!";
        "#,
        tele_id as i64
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(res) => {
            let exists: bool = res.exists;

            if exists {
                log::info!("User with telegram id: ({}) exists", tele_id);
            } else {
                log::info!("User with telegram id: ({}) does not exist", tele_id);
            }

            Ok(exists)
        }
        Err(e) => {
            log::error!("Error querying user: {}", e);

            Err(e)
        }
    }
}

pub(crate) async fn user_exists_ops_name(conn: &PgPool, ops_name: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM usrs
            WHERE usrs.ops_name = $1 AND is_valid = TRUE
        ) AS "exists!";
        "#,
        ops_name
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            let exists = res.exists;

            if exists {
                log::info!("User with ops_name: ({}) exists", ops_name);
            } else {
                log::info!("User with ops_name: ({}) does not exist", ops_name);
            }

            Ok(exists)
        }
        Err(e) => {
            log::error!("Error querying user: {}", e);

            Err(e)
        }
    }
}

pub(crate) async fn get_user_by_tele_id(conn: &PgPool, tele_id: u64) -> Result<Usr, sqlx::Error> {
    let result = sqlx::query_as!(
        Usr,
        r#"
        SELECT
            usrs.id AS id,
            usrs.tele_id AS tele_id,
            usrs.name AS name,
            usrs.ops_name AS ops_name,
            usrs.usr_type AS "usr_type: _",
            usrs.role_type AS "role_type: _",
            usrs.admin AS admin,
            usrs.created AS created,
            usrs.updated AS updated
        FROM usrs
        WHERE usrs.tele_id = $1 AND usrs.is_valid = TRUE;
        "#,
        tele_id as i64
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            log::info!("Get user by tele_id: {}", tele_id);

            Ok(res)
        }
        Err(e) => {
            log::error!("Error getting user: {}", e);

            Err(e)
        }
    }
}

pub(crate) async fn get_user_by_ops_name(conn: &PgPool, ops_name: &str) -> Result<Usr, sqlx::Error> {
    let result = sqlx::query_as!(
        Usr,
        r#"
        SELECT
            usrs.id AS id,
            usrs.tele_id AS tele_id,
            usrs.name AS name,
            usrs.ops_name AS ops_name,
            usrs.usr_type AS "usr_type: _",
            usrs.role_type AS "role_type: _",
            usrs.admin AS admin,
            usrs.created AS created,
            usrs.updated AS updated
        FROM usrs
        WHERE usrs.ops_name = $1 AND usrs.is_valid = TRUE;
        "#,
        ops_name
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            log::info!("Get user by ops_name: {}", ops_name);

            Ok(res)
        }
        Err(e) => {
            log::error!("Error getting user: {}", e);

            Err(e)
        }
    }
}

pub(crate) async fn add_user(
    conn: &PgPool,
    tele_id: u64,
    name: String,
    ops_name: String,
    role_type: RoleType,
    user_type: UsrType,
    admin: bool
) -> Result<Usr, sqlx::Error> {
    let result = sqlx::query_as!(
        Usr,
        r#"
        INSERT INTO usrs (tele_id, name, ops_name, role_type, usr_type, admin)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING
            id,
            tele_id,
            name,
            ops_name,
            usr_type AS "usr_type: _",
            role_type AS "role_type: _",
            admin,
            created,
            updated
        "#,
        tele_id as i64,
        name,
        ops_name,
        role_type as RoleType,  // Casting the RoleType enum
        user_type as UsrType,   // Casting the UsrType enum
        admin
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(user) => {
            log::info!("Added user with tele_id: {}", tele_id);
            Ok(user)
        }
        Err(e) => {
            log::error!("Error adding user: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn remove_user_by_tele_id(conn: &PgPool, tele_id: u64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE usrs
        SET is_valid = FALSE
        WHERE tele_id = $1 AND is_valid = TRUE;
        "#,
        tele_id as i64
    )
        .execute(conn)
        .await;

    match result {
        Ok(query_result) => {
            if query_result.rows_affected() == 1 {
                log::info!("Soft deleted user with tele_id: {}", tele_id);
                Ok(true)
            } else {
                log::warn!("No user found with tele_id: {}", tele_id);
                Ok(false)
            }
        }
        Err(e) => {
            log::error!("Error soft deleting user: {}", e);
            Err(e)
        }
    }
}

pub(crate) async fn remove_user_by_uuid(conn: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        UPDATE usrs
        SET is_valid = FALSE
        WHERE id = $1 AND is_valid = TRUE;
        "#,
        id
    )
        .execute(conn)
        .await;

    match result {
        Ok(query_result) => {
            if query_result.rows_affected() == 1 {
                log::info!("Successfully soft-deleted user with id: {}", id);
                Ok(true)
            } else {
                log::warn!("No user found with id: {}", id);
                Ok(false)
            }
        }
        Err(e) => {
            log::error!("Error soft deleting user by id: {}", e);
            Err(e)
        }
    }
}


pub(crate) async fn update_user(
    conn: &PgPool,
    user_details: &Usr,
) -> Result<Usr, sqlx::Error> {
    let result = sqlx::query_as!(
        Usr,
        r#"
        UPDATE usrs
        SET
            tele_id = $1,
            name = $2,
            ops_name = $3,
            usr_type = $4,
            role_type = $5,
            admin = $6
        WHERE id = $7 AND is_valid = TRUE
        RETURNING
            id,
            tele_id,
            name,
            ops_name,
            usr_type AS "usr_type: _",
            role_type AS "role_type: _",
            admin,
            created,
            updated
        "#,
        user_details.tele_id as i64,
        &user_details.name,
        &user_details.ops_name,
        user_details.usr_type.clone() as UsrType,
        user_details.role_type.clone() as RoleType,
        user_details.admin,
        user_details.id,
    )
        .fetch_one(conn)
        .await;

    match result {
        Ok(user) => {
            log::info!("Updated user with id: {}", user_details.id);
            Ok(user)
        }
        Err(e) => {
            log::error!("Error updating user: {}", e);
            Err(e)
        }
    }
}