use sqlx::PgPool;

pub(crate) async fn apply_user(conn: &PgPool, tele_id: i64, name: String) -> bool {
    let result = sqlx::query!(
        r#"
        INSERT INTO apply (tele_id, name)
        VALUES ($1, $2)
        ON CONFLICT (tele_id) DO UPDATE
            SET name = EXCLUDED.name;
        "#,
        tele_id,
        name
    )
    .execute(conn)
    .await;

    match result {
        Ok(res) => {
            log::info!("User with id: ({}) and name: ({}) applied", tele_id, name);
            log::debug!("Apply {:?}", res);

            true
        }
        Err(e) => {
            log::error!("Error inserting user application: {}", e);

            false
        }
    }
}
