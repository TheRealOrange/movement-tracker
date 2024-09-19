mod bot;
mod controllers;
pub(crate) mod types;
mod utils;

use std::env;
use sqlx::PgPool;
use crate::controllers::db;
use teloxide::prelude::*;
use crate::types::{RoleType, UsrType};

#[tokio::main]
async fn main() {
    // Load environment variables from .env file.
    dotenvy::dotenv().expect(".env file not found");

    pretty_env_logger::init();
    log::info!("Initiating connection to database...");
    let conn_pool = db::init_db().await.expect("Failed to initialize database");

    // Add the default user
    match add_default_user_from_env(&conn_pool).await {
        Ok(user) => {
            println!("Default user added or already exists: {:?}", user);
        }
        Err(e) => {
            eprintln!("Error adding default user: {}", e);
        }
    }

    log::info!("Starting command bot...");
    bot::init_bot(conn_pool).await;
}


pub(crate) async fn add_default_user_from_env(conn: &PgPool) -> Result<(), sqlx::Error> {
    // Check if DEFAULT_TELEGRAM_ID is set
    let tele_id_env = match env::var("DEFAULT_TELEGRAM_ID") {
        Ok(val) => val,
        Err(_) => {
            log::info!("DEFAULT_TELEGRAM_ID is not set. No default user will be added.");
            return Ok(());
        }
    };

    let tele_id: u64 = match tele_id_env.parse() {
        Ok(id) => id,
        Err(_) => {
            log::error!("Invalid DEFAULT_TELEGRAM_ID value: {}", tele_id_env);
            return Ok(());
        }
    };

    // Optionally, read the name from environment variable
    let name = env::var("DEFAULT_USER_NAME").unwrap_or_else(|_| "Default User".to_string());

    // Read the ops_name from environment variable
    let ops_name = env::var("DEFAULT_OPS_NAME").unwrap_or_else(|_| "default_ops_name".to_string());

    // Assume default details
    let role_type = RoleType::PILOT; // Adjust as needed
    let usr_type = UsrType::ACTIVE;  // Set usr_type to Active
    let admin = true;           // Set admin to true

    // Check if the user already exists
    match controllers::user::user_exists_tele_id(conn, tele_id).await {
        Ok(exists) => {
            if exists {
                log::info!("User with Telegram ID {} already exists.", tele_id);
            } else {
                log::info!("User with Telegram ID {} does not exist. Adding default user.", tele_id);
                // Add the user with default details
                controllers::user::add_user(
                    conn,
                    tele_id,
                    name,
                    ops_name,
                    role_type,
                    usr_type,
                    admin,
                ).await?;
            }
            Ok(())
        }
        Err(e) => {
            log::error!("Error checking if user exists: {}", e);
            Err(e)
        }
    }
}