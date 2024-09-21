mod bot;
mod controllers;
pub(crate) mod types;
mod utils;
mod notifier;

use std::env;
use std::path::Path;
use sqlx::PgPool;
use crate::controllers::db;
use teloxide::prelude::*;
use crate::types::{RoleType, UsrType};
use warp::Filter;

#[tokio::main]
async fn main() {
    // Check if .env file exists
    if Path::new(".env").exists() {
        dotenvy::dotenv().expect("Failed to load .env file");
        println!("Loaded environment variables from .env file");
    } else {
        println!("No .env file found, using environment variables");
    }

    pretty_env_logger::init();
    log::info!("Initiating connection to database...");
    let conn_pool = db::init_db().await.expect("Failed to initialize database");

    // Add the default user
    match add_default_user_from_env(&conn_pool).await {
        Ok(()) => {
            log::info!("Default user added or already exists.");
        }
        Err(e) => {
            log::error!("Error adding default user: {}", e);
        }
    }

    log::info!("Starting bot and scheduled notifications...");

    let bot = Bot::from_env();

    // Clone the bot and conn_pool for the notifier task
    let notifier_bot = bot.clone();
    let notifier_conn_pool = conn_pool.clone();

    // Start the notifier task
    tokio::spawn(async move {
        notifier::scheduled::start_notifier(notifier_bot, notifier_conn_pool).await;
    });

    // Define the health check route
    let health_route = warp::path("health").map(|| "OK");

    // Start the health check server on a separate task
    tokio::spawn(async move {
        warp::serve(health_route)
            .run(([0, 0, 0, 0], 8080))
            .await;
    });

    // Start the bot
    bot::init_bot(bot, conn_pool).await;
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
                match controllers::user::add_user(
                    conn,
                    tele_id,
                    name,
                    ops_name,
                    role_type,
                    usr_type,
                    admin,
                ).await {
                    Ok(user) => {
                        // Set default notification settings
                        match controllers::notifications::update_notification_settings(
                            &conn,
                            user.tele_id, // Assuming chat_id == tele_id
                            Some(true),  // notif_system
                            Some(true),  // notif_register
                            None,        // notif_availability
                            Some(true),  // notif_plan
                            Some(true),  // notif_conflict
                        ).await {
                            Ok(_) => {
                                log::info!("Default notification settings configured for default user {}", user.ops_name);
                            }
                            Err(_) => {
                                log::error!("Failed to configure default notification settings for default user {}", user.ops_name);
                            }
                        }
                    }
                    Err(_) => {
                        log::error!("Failed to add default user")
                    }
                };
            }
            Ok(())
        }
        Err(e) => {
            log::error!("Error checking if user exists: {}", e);
            Err(e)
        }
    }
}