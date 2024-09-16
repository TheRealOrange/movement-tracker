use teloxide::{prelude::*, utils::command::BotCommands};
use crate::bot::state::State;
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
    #[command(description = "Cancel current action")]
    Cancel,
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub(super) enum PrivilegedCommands {
    #[command(description = "Modify user attributes")]
    User,
}

pub(super) async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Commands::descriptions().to_string()).await?;
    Ok(())
}

pub(super) async fn cancel(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    log::debug!("Command: cancel, Message: {:?}", msg);
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