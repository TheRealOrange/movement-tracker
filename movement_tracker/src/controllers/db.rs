use sqlx::migrate::{MigrateDatabase, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres};
use std::env;
use std::error::Error;

// Define a static migrator that looks for migration files in the "migrations" folder.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub(crate) async fn init_db() -> Result<PgPool, Box<dyn Error>> {
    log::info!("Establishing connection to db");
    let db_url: &str = &env::var("DATABASE_URL").expect("DATABASE_URL is not set");
    let mut max_connections: u32 = 5;
    match &env::var("MAX_DB_CONNECTIONS") {
        Ok(num) => {
            max_connections = num.parse().expect("Invalid MAX_DB_CONNECTIONS value");
            log::info!("Max connections: {}", max_connections);
        }
        Err(error) => {
            log::warn!("MAX_DB_CONNECTIONS is not set, using default value of {} max connections.", max_connections);
            log::debug!("error: {}", error);
        }
    }

    match Postgres::database_exists(db_url).await {
        Ok(exists) => {
            if exists {
                log::info!("Database exists");
            } else {
                panic!("Database does not exist!")
            }
        },
        Err(error) => panic!("error: {}", error),
    }

    let pool;
    match PgPoolOptions::new()
        .min_connections(max_connections)
        .max_connections(max_connections)
        .test_before_acquire(false)
        .connect(db_url).await {
        Ok(p) => {
            pool = p;
            log::info!("Database connected");

            // Run the migrations
            match MIGRATOR.run(&pool).await {
                Ok(_) => {
                    log::info!("Database migrations completed");
                }
                Err(error) => panic!("Fatal error during database migration: {}", error),
            };
        }
        Err(error) => panic!("Unable to connect to database: {}", error),
    }

    Ok(pool)
}