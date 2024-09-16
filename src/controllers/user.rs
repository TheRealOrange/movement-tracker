use crate::types::{Usr, wrap_to_i64, RoleType, UsrType};
use sqlx::PgPool;
use teloxide::types::CountryCode::SO;

pub(crate) async fn user_exists_tele_id(conn: &PgPool, tele_id: u64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM usrs
            WHERE usrs.tele_id = $1
        ) AS "exists!";
        "#,
        wrap_to_i64(tele_id)
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

async fn user_exists_ops_name(conn: &PgPool, ops_name: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM usrs
            WHERE usrs.ops_name = $1
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
        WHERE usrs.tele_id = $1;
        "#,
        wrap_to_i64(tele_id)
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

async fn get_user_by_ops_name(conn: &PgPool, ops_name: &str) -> Result<Usr, sqlx::Error> {
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
        WHERE usrs.ops_name = $1;
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
        wrap_to_i64(tele_id),  // Assuming you have a function for converting u64 to i64
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
