mod bot;
mod controllers;

use crate::bot::commands;
use crate::controllers::db;
use teloxide::prelude::*;

#[tokio::main]
async fn main() {
    // Load environment variables from .env file.
    dotenvy::dotenv().expect(".env file not found");

    pretty_env_logger::init();
    log::info!("Initiating connection to database...");
    db::init_db().await.expect("Failed to initialize database");

    log::info!("Starting command bot...");

    let bot = Bot::from_env();

    commands::Command::repl(bot, commands::answer).await;
}
