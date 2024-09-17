use teloxide::{prelude::*, utils::command::BotCommands};
use crate::bot::state::State;
use crate::log_endpoint_hit;
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

pub(super) async fn help(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "help", "Command", msg);
    bot.send_message(msg.chat.id, Commands::descriptions().to_string()).await?;
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