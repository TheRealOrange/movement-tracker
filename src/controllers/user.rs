use crate::controllers::model::Usr;
use sqlx::PgPool;

async fn user_exists_tele_id(conn: &PgPool, tele_id: i64) -> (bool, bool) {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM usrs
            WHERE usrs.tele_id = $1
        );
        "#,
        tele_id
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            let exists: bool = res.exists.unwrap_or(false);

            if exists {
                log::info!("User with telegram id: ({}) exists", tele_id);
            } else {
                log::info!("User with telegram id: ({}) does not exist", tele_id);
            }

            (true, exists)
        }
        Err(e) => {
            log::error!("Error querying user: {}", e);

            (false, false)
        }
    }
}

async fn user_exists_ops_name(conn: &PgPool, ops_name: &str) -> (bool, bool) {
    let result = sqlx::query!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM usrs
            WHERE usrs.ops_name = $1
        );
        "#,
        ops_name
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            let exists: bool = res.exists.unwrap_or(false);

            if exists {
                log::info!("User with ops_name: ({}) exists", ops_name);
            } else {
                log::info!("User with ops_name: ({}) does not exist", ops_name);
            }

            (true, exists)
        }
        Err(e) => {
            log::error!("Error querying user: {}", e);

            (false, false)
        }
    }
}

async fn get_user_by_tele_id(conn: &PgPool, tele_id: i64) -> (bool, Option<Usr>) {
    let result = sqlx::query_as!(
        Usr,
        r#"
        SELECT
            usrs.id AS id,
            usrs.tele_id AS tele_id,
            usrs.name AS name,
            usrs.ops_name AS ops_name,
            usrs.usr_type AS "usr_type: _",
            usrs.admin AS admin,
            usrs.created AS created,
            usrs.updated AS updated
        FROM usrs
        WHERE usrs.tele_id = $1;
        "#,
        tele_id
    )
    .fetch_one(conn)
    .await;

    match result {
        Ok(res) => {
            log::info!("Get user by tele_id: {}", tele_id);

            (true, Some(res))
        }
        Err(e) => {
            log::error!("Error getting user: {}", e);

            (false, None)
        }
    }
}

async fn get_user_by_ops_name(conn: &PgPool, ops_name: &str) -> (bool, Option<Usr>) {
    let result = sqlx::query_as!(
        Usr,
        r#"
        SELECT
            usrs.id AS id,
            usrs.tele_id AS tele_id,
            usrs.name AS name,
            usrs.ops_name AS ops_name,
            usrs.usr_type AS "usr_type: _",
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

            (true, Some(res))
        }
        Err(e) => {
            log::error!("Error getting user: {}", e);

            (false, None)
        }
    }
}
