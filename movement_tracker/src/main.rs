mod bot;
mod controllers;
pub(crate) mod types;
mod utils;
mod notifier;
mod healthcheck;

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use sys_locale::get_locale;

use axum::{Extension, Router};
use axum::routing::get;
use tower_http::{trace::TraceLayer};
use tower::ServiceBuilder;

use chrono_tz::{Tz, TZ_VARIANTS};
use once_cell::sync::Lazy;
use sqlx::PgPool;

use teloxide::prelude::*;
use tokio::sync::Mutex;

use crate::controllers::db;
use crate::types::{RoleType, UsrType};
use crate::healthcheck::monitor::CurrentHealthStatus;

#[derive(Clone)]
struct AppState {
    db_pool: PgPool,
    notifier_status: Arc<Mutex<bool>>,
    audit_status: Arc<Mutex<bool>>,
    bot_health: Arc<Mutex<bool>>,
    bot: Bot,
    bot_health_check_active: bool,
    health_status: Arc<Mutex<CurrentHealthStatus>>
}

pub(crate) static APP_TIMEZONE: Lazy<Tz> = Lazy::new(get_timezone);
#[macro_export]
macro_rules! now {
    () => {{
        Utc::now().with_timezone(&*APP_TIMEZONE)
    }};
}

#[tokio::main]
async fn main() {
    // Check if .env file exists
    if Path::new(".env").exists() {
        dotenvy::dotenv().expect("Failed to load .env file");
        log::info!("Loaded environment variables from .env file");
    } else {
        log::warn!("No .env file found, using environment variables");
    }

    pretty_env_logger::init_timed();
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

    // Initialize the Telegram Bot
    let bot = Bot::from_env();

    // Check if BOT_HEALTH_CHECK_CHAT_ID is set at startup
    let bot_health_check_active = match env::var("BOT_HEALTH_CHECK_CHAT_ID") {
        Ok(chat_id_env) => {
            match chat_id_env.parse() {
                Ok(id) => {
                    match bot.get_chat(ChatId(id)).await {
                        Ok(chat) => {
                            log::info!("Chat validation successful for chat ID ({}).", chat.id);
                            log::info!("BOT_HEALTH_CHECK_CHAT_ID is set and valid. Bot health check will be active.");
                            true
                        }
                        Err(e) => {
                            log::error!("Unable to retrieve chat with ID ({}). Please ensure the bot has been added to the healthcheck chat: {}", id, e);
                            false
                        }
                    }
                }
                Err(_) => {
                    log::error!("Invalid BOT_HEALTH_CHECK_CHAT_ID value: {}", chat_id_env);
                    false
                }
            }
        }
        Err(_) => {
            log::warn!("BOT_HEALTH_CHECK_CHAT_ID is not set. Bot health check will be inactive.");
            false
        }
    };

    // Initialize the Shared Application State
    let app_state = Arc::new(AppState {
        db_pool: conn_pool.clone(),
        notifier_status: Arc::new(Mutex::new(true)),
        audit_status: Arc::new(Mutex::new(true)),
        bot_health: Arc::new(Mutex::new(true)),
        bot: bot.clone(),
        bot_health_check_active, // Set bot health check active flag
        health_status: Arc::new(Mutex::new(CurrentHealthStatus::new()))
    });

    // Clone the State for Notifier and Audit Tasks
    let notifier_app_state = app_state.clone();
    let audit_app_state = app_state.clone();

    // Clone bot for the notifier task
    let notifier_bot = bot.clone();

    // Spawn the Notifier Task
    tokio::spawn(async move {
        if let Err(e) = notifier::scheduled::start_notifier(notifier_bot, notifier_app_state.clone()).await {
            log::error!("Notifier task encountered an error: {}", e);
            let mut notifier = notifier_app_state.notifier_status.lock().await;
            *notifier = false;
        }
    });

    // Spawn the Audit Task
    tokio::spawn(async move {
        if let Err(e) = healthcheck::audit::start_audit_task(audit_app_state.clone()).await {
            log::error!("Audit task encountered an error: {}", e);
            let mut audit = audit_app_state.audit_status.lock().await;
            *audit = false;
        }
    });

    // Spawn the Health Monitoring Task
    let health_monitor_state = app_state.clone();
    tokio::spawn(async move {
        healthcheck::monitor::start_health_monitor(health_monitor_state).await;
    });

    // Start Health Check Server on Port 8080
    let health_route = Router::new()
        .route("/health", get(healthcheck::handler::health_check_handler))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(Extension(app_state.clone()))
        );  // Add shared state;

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080)).await.unwrap();
        axum::serve(listener, health_route).await.expect("Failed to run health check server");
    });

    // Determine which listener to use based on the environment variable
    if let Ok(webhook_port_str) = env::var("WEBHOOK_PORT") {
        // Webhook Setup

        // Parse the webhook port
        let webhook_port: u16 = webhook_port_str
            .parse()
            .expect("WEBHOOK_PORT must be a valid u16 integer");

        // Retrieve the HOST environment variable
        let host = std::env::var("HOST")
            .expect("HOST environment variable must be set for webhooks");

        // Construct the webhook URL
        let webhook_url = format!("https://{host}/webhook")
            .parse()
            .expect("Invalid webhook URL");

        // Define the address to bind the webhook server
        let addr = ([0, 0, 0, 0], webhook_port).into();

        log::info!("Setting up webhook at {}", webhook_url);

        // Initialize the Webhook Listener Using Axum
        let listener = teloxide::update_listeners::webhooks::axum(
            bot.clone(),
            teloxide::update_listeners::webhooks::Options::new(addr, webhook_url)
        )
            .await
            .expect("Failed to set up webhook");
        
        bot::init_bot(bot, conn_pool, listener, app_state.clone()).await;
    } else {
        // Initialize polling listener
        log::info!("WEBHOOK_PORT not set. Falling back to long polling.");

        let listener = teloxide::update_listeners::polling_default(bot.clone()).await;
        bot::init_bot(bot, conn_pool, listener, app_state.clone()).await;
    };
}

fn get_timezone() -> Tz {
    // Check for the TIMEZONE environment variable
    if let Ok(tz_name) = env::var("TIMEZONE") {
        // If TIMEZONE is set, try to parse it into a valid Tz
        if let Ok(tz) = tz_name.parse::<Tz>() {
            log::info!("Using specified timezone: {}", tz_name);
            return tz;
        } else {
            log::error!("Invalid TIMEZONE provided: {} Trying to use system local timezone.", tz_name);
        }
    }

    // If TIMEZONE is not set or invalid, try to get the system's local timezone
    if let Some(locale) = get_locale() {
        for &tz in &TZ_VARIANTS {
            if locale.contains(tz.name()) {
                log::warn!("Using system local timezone: {}", tz);
                return tz;
            }
        }
    }
    
    log::warn!("Falling back to UTC timezone");
    // Fallback to UTC if everything else fails
    chrono_tz::UTC
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