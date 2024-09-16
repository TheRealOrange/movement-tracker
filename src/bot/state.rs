use sqlx::PgPool;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*, RequestError};
use teloxide::dispatching::{dialogue, UpdateHandler};
use teloxide::dptree::{case, endpoint};
use teloxide::payloads::SendMessage;
use teloxide::requests::JsonRequest;
use teloxide::types::{MessageId, User};
use super::{send_msg, HandlerResult, MyDialogue};
use super::commands::{cancel, Commands, help, PrivilegedCommands};

use super::register::{register, register_complete, register_name, register_ops_name, register_role, register_type};
use super::user::user;
use crate::controllers;
use crate::types::{RoleType, Usr, UsrType};

#[derive(Clone, Default)]
pub(super) enum State {
    #[default]
    Start,
    Register,
    RegisterRole,
    RegisterType {
        role_type: RoleType,
    },
    RegisterName {
        role_type: RoleType,
        user_type: UsrType,
    },
    RegisterOpsName {
        role_type: RoleType,
        user_type: UsrType,
        name: String,
    },
    RegisterComplete {
        role_type: RoleType,
        user_type: UsrType,
        name: String,
        ops_name: String,
    },
    Movement,
    EditMovement,
    AddMovement,
    AddMovementDetails {
        details: String,
    },
    Availability,
    EditAvailability,
    AddAvailability,
    AddAvailabilityDetails {
        details: String,
    },
    User,
    UserSelect,
    ChangeOpsName {
        ops_name: String,
    },
    ChangeUserType {
        user_type: String
    },
    ErrorState
}

pub(super) fn schema() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    let command_handler = teloxide::filter_command::<Commands, _>()
        .branch(
            case![State::Start]
                .branch(case![Commands::Help].endpoint(help))
                .branch(case![Commands::Register].endpoint(register)),
        )
        .branch(case![Commands::Cancel].endpoint(cancel));

    let admin_command_handler = teloxide::filter_command::<PrivilegedCommands, _>()
        .branch(
            case![State::Start]
                .branch(case![PrivilegedCommands::User].endpoint(user)),
        );

    let message_handler = Update::filter_message()
        .branch(case![State::ErrorState].endpoint(error_state))
        .branch(command_handler)
        .branch(dptree::filter_async(check_admin)
            .branch(admin_command_handler))
        .branch(case![State::RegisterName { role_type, user_type }].endpoint(register_name))
        .branch(case![State::RegisterOpsName { role_type, user_type, name }].endpoint(register_ops_name))
        .branch(case![State::RegisterComplete { role_type, user_type, name, ops_name }].endpoint(register_complete));

    let callback_query_handler = Update::filter_callback_query()
        .branch(case![State::RegisterRole].endpoint(register_role))
        .branch(case![State::RegisterType { role_type }].endpoint(register_type));

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
        .branch(endpoint(invalid_state))
}

// Function to handle "server is overloaded" message if connection acquisition fails
async fn reply_server_overloaded(bot: &Bot, msg: &Message) {
    send_msg(
        bot.send_message(msg.chat.id, "Server is overloaded. Please try again later.")
        .reply_to_message_id(msg.id),
        match msg.from() {
            Some(from) => &(from.username),
            _ => &None
        }
    ).await;
}

async fn check_admin(bot: Bot, msg: Message, pool: PgPool) -> bool {
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from() {
        user
    } else {
        return false;
    };

    // Helper function to log and return false on any errors
    let handle_error = || async {
        send_msg(
            bot.send_message(msg.chat.id, "Error occurred accessing the database")
                .reply_to_message_id(msg.id),
            &(user.username)
        ).await;
        false
    };

    // Check if the telegram ID exists in the database
    match controllers::user::user_exists_tele_id(&pool, user.id.0).await{
        Ok(true) => {
            // Check if the user is an admin
            match controllers::user::get_user_by_tele_id(&pool, user.id.0).await {
                Ok(retrieved_usr) => {
                    retrieved_usr.admin
                }
                Err(_) => handle_error().await,
            }
        },
        Ok(false) => false,
        Err(_) => handle_error().await
    }
}

async fn error_state(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    log::info!("Reached error state");
    send_msg(
        bot.send_message(msg.chat.id, "Error occurred, returning to start!")
            .reply_to_message_id(msg.id),
        &None
    ).await;
    dialogue.update(State::Start).await?;
    Ok(())
}

async fn invalid_state(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    log::info!("Reached an invalid state! (how did you do this?)");
    send_msg(
        bot.send_message(msg.chat.id, "Error occurred, returning to start!")
            .reply_to_message_id(msg.id),
        &None
    ).await;
    dialogue.update(State::Start).await?;
    Ok(())
}