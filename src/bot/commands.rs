use sqlx::PgPool;
use teloxide::{prelude::*, utils::command::BotCommands};
use teloxide::types::ReplyParameters;
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit};
use super::{send_msg, MyDialogue};
use super::HandlerResult;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub(super) enum Commands {
    #[command(description = "Display this help text")]
    Help,
    #[command(description = "Register to use this bot")]
    Register,
    #[command(description = "Indicate your availability (for SANS)")]
    Availability,
    #[command(description = "Add information about your movement")]
    Movement,
    #[command(description = "Cancel current action")]
    Cancel,
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub(super) enum PrivilegedCommands {
    #[command(description = "Approve registration requests")]
    Approve,
    #[command(description = "Modify user attributes")]
    User,
}

pub(super) async fn help(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "help", "Command", msg);
    let mut help_str = Commands::descriptions().to_string();

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        send_msg(
            bot.send_message(msg.chat.id, help_str),
            &None
        ).await;
        return Ok(());
    };

    // Helper function to log and return false on any errors
    let handle_error = || async {
        send_msg(
            bot.send_message(msg.chat.id, "Error occurred accessing the database")
                .reply_parameters(ReplyParameters::new(msg.id)),
            &(user.username)
        ).await;
    };

    // Check if the telegram ID exists in the database
    match controllers::user::user_exists_tele_id(&pool, user.id.0).await{
        Ok(true) => {
            // Check if the user is an admin
            match controllers::user::get_user_by_tele_id(&pool, user.id.0).await {
                Ok(retrieved_usr) => {
                    if retrieved_usr.admin == true {
                        help_str = format!("{}\n\nAdmin Commands:\n{}", help_str, PrivilegedCommands::descriptions().to_string());
                    }
                }
                Err(_) => handle_error().await,
            }
        },
        Ok(false) => (),
        Err(_) => handle_error().await
    }

    send_msg(
        bot.send_message(msg.chat.id, help_str),
        &(user.username)
    ).await;
    
    Ok(())
}

pub(super) async fn cancel(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "cancel", "Command", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from() {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };
    
    send_msg(
        bot.send_message(msg.chat.id, "Cancelling, returning to start!"),
        &(user.username)
    ).await;
    
    dialogue.update(State::Start).await?;
    Ok(())
}