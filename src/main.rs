mod bot;
mod controllers;
pub(crate) mod types;

use crate::controllers::db;
use teloxide::prelude::*;

#[tokio::main]
async fn main() {
    // Load environment variables from .env file.
    dotenvy::dotenv().expect(".env file not found");

    pretty_env_logger::init();
    log::info!("Initiating connection to database...");
    let conn_pool = db::init_db().await.expect("Failed to initialize database");

    log::info!("Starting command bot...");
    bot::init_bot(conn_pool).await;
}
