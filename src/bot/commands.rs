use super::HandlerResult;
use super::{send_msg, MyDialogue};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit};
use sqlx::PgPool;
use teloxide::types::{BotCommand, BotCommandScope, ChatKind, MenuButton, Recipient};
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub(super) enum Commands {
    #[command(description = "Display this help text")]
    Start,
    #[command(description = "Display this help text")]
    Help,
    #[command(description = "Register to use this bot")]
    Register,
    #[command(description = "Indicate or modify your availability (for SANS)")]
    Availability,
    #[command(description = "View upcoming planned")]
    Upcoming,
    // #[command(description = "Add information about your movement")]
    // Movement,
    #[command(description = "View information about the future availability")]
    Forecast,
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
    #[command(description = "Modify user attributes, /user <OPS NAME>")]
    User {
        ops_name: String
    },
    #[command(description = "Plan for flight, /plan <OPS NAME> or /plan <date>")]
    Plan {
        ops_name_or_date: String
    },
    #[command(description = "Track SAF100")]
    SAF100,
    #[command(description = "Edit notification settings for current chat")]
    Notify
}

// Function to set commands and menu buttons
pub(super) async fn set_menu_buttons(bot: Bot, chat_id: ChatId, user_id: UserId, is_admin: bool, is_public: bool) {
    let mut commands: Vec<BotCommand> = Commands::bot_commands().to_vec();

    // If the user is an admin, append privileged commands
    if is_admin && !is_public {
        let mut privileged_commands = PrivilegedCommands::bot_commands().to_vec();
        commands.append(&mut privileged_commands);
    }
    
    if is_public {
        // Set the combined commands for the chat
        match bot.set_my_commands(commands).scope(BotCommandScope::ChatMember { chat_id: Recipient::Id(chat_id), user_id }).await {
            Ok(_) => {}
            Err(err) => {
                // Log the error if setting commands fails
                log::error!("Failed to set commands for public chat: {:?}", err);
            }
        };
    } else {
        // Set the combined commands for the user
        match bot.set_my_commands(commands).scope(BotCommandScope::Chat { chat_id: Recipient::Id(chat_id) }).await {
            Ok(_) => {}
            Err(err) => {
                // Log the error if setting commands fails
                log::error!("Failed to set commands: {:?}", err);
            }
        };
        
        // Set the chat menu button to show commands
        match bot.set_chat_menu_button()
            .menu_button(MenuButton::Commands)
            .chat_id(chat_id) // Here you use MenuButton::Commands
            .await {
            Ok(_) => {}
            Err(err) => {
                // Log the error if setting the menu button fails
                log::error!("Failed to set chat menu button: {:?}", err);
            }
        }
    }
}

pub(super) async fn help(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "help", "Command", msg);
    let mut help_str = Commands::descriptions().to_string();

    let user = if let Some(user) = msg.from {
        user
    } else {
        send_msg(
            bot.send_message(msg.chat.id, help_str),
            &None
        ).await;
        return Ok(());
    };

    let is_admin = match controllers::user::get_user_by_tele_id(&pool, user.id.0).await {
        Ok(retrieved_usr) => retrieved_usr.admin,
        Err(_) => false,
    };

    // Determine the kind of chat the message was sent in
    let is_public_chat = matches!(&msg.chat.kind, ChatKind::Public(_));

    // Append admin commands to the help message if the user is an admin
    if is_admin && !is_public_chat {
        help_str = format!("{}\n\nAdmin Commands:\n{}", help_str, PrivilegedCommands::descriptions());
    }

    send_msg(
        bot.send_message(msg.chat.id, help_str),
        &(user.username)
    ).await;
    
    set_menu_buttons(bot, dialogue.chat_id(), user.id, is_admin, is_public_chat).await;

    Ok(())
}

pub(super) async fn cancel(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "cancel", "Command", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
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