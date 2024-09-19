use std::str::FromStr;
use sqlx::PgPool;
use strum::{IntoEnumIterator, ParseError};
use teloxide::Bot;
use teloxide::prelude::Message;
use teloxide::dispatching::dialogue::{GetChatId, InMemStorageError};
use teloxide::payloads::SendMessageSetters;
use teloxide::requests::Requester;
use teloxide::types::{CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, ReplyParameters};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit};
use crate::types::{RoleType, UsrType};
use super::{handle_error, send_msg, HandlerResult, MyDialogue};

async fn display_role_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let roles = RoleType::iter()
        .map(|role| InlineKeyboardButton::callback(role.as_ref(), role.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Please select your role:")
            .reply_markup(InlineKeyboardMarkup::new([roles])),
        username
    ).await;
}

async fn display_user_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let usrtypes = UsrType::iter()
        .map(|usrtype| InlineKeyboardButton::callback(usrtype.as_ref(), usrtype.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Please select your status:")
            .reply_markup(InlineKeyboardMarkup::new([usrtypes])),
       username
    ).await;
}

async fn display_register_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    send_msg(
        bot.send_message(chat_id, "Type your full name:"),
        username,
    ).await;
}

async fn display_register_ops_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    send_msg(
        bot.send_message(chat_id, "Type your OPS NAME:"),
        username,
    ).await;
}

async fn display_register_confirmation(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let confirm = ["YES", "NO"]
        .map(|product| InlineKeyboardButton::callback(product, product));

    send_msg(
        bot.send_message(chat_id, "Confirm registration?")
            .reply_markup(InlineKeyboardMarkup::new([confirm])),
        username
    ).await;
}

pub(super) async fn register(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "register", "Command", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Check if the telegram ID exists in the database
    match controllers::user::user_exists_tele_id(&pool, user.id.0).await{
        Ok(true) => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "You have already registered")
                    .reply_parameters(ReplyParameters::new(msg.id)),
                &user.username,
            ).await;
            dialogue.update(State::Start).await?;
        },
        Ok(false) => {
            display_role_types(&bot, dialogue.chat_id(), &user.username).await;
            log::debug!("Transitioning to RegisterRole");
            dialogue.update(State::RegisterRole).await?;
        },
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }

    Ok(())
}

pub(super) async fn register_role(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "register_role", "Callback", q);

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;

            display_role_types(&bot, dialogue.chat_id(), &q.from.username).await;
        }
        Some(role) => {
            log::debug!("Received input: {:?}", &role);
            match RoleType::from_str(&role) {
                Ok(role_enum) => {
                    log::debug!("Selected role: {:?}", role_enum);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Selected role: {}", role_enum.as_ref())),
                        &q.from.username,
                    ).await;

                    display_user_types(&bot, dialogue.chat_id(), &q.from.username).await;
                    log::debug!("Transitioning to RegisterType with RoleType: {:?}", role_enum);
                    dialogue.update(State::RegisterType { role_type: role_enum }).await?;
                }
                Err(e) => {
                    log::error!("Invalid role type received: {}", e);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                        &q.from.username,
                    ).await;
                    display_role_types(&bot, dialogue.chat_id(), &q.from.username).await;
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn register_type(
    bot: Bot,
    dialogue: MyDialogue,
    role_type: RoleType,
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_type", "Callback", q, 
        "RoleType" => role_type
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;

            display_user_types(&bot, dialogue.chat_id(), &q.from.username).await;
        }
        Some(usrtype) => {
            log::debug!("Received input: {:?}", &usrtype);
            match UsrType::from_str(&usrtype) {
                Ok(user_type_enum) => {
                    log::debug!("Selected user type: {:?}", user_type_enum);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Selected user type: {}", user_type_enum.as_ref())),
                        &q.from.username,
                    ).await;
                    display_register_name(&bot, dialogue.chat_id(), &q.from.username).await;
                    log::debug!("Transitioning to RegisterName with RoleType: {:?}, UsrType: {:?}", role_type, user_type_enum);
                    dialogue.update(State::RegisterName { role_type, user_type: user_type_enum }).await?;
                }
                Err(e) => {
                    log::error!("Invalid user type received: {}", e);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                        &q.from.username,
                    ).await;
                    display_user_types(&bot, dialogue.chat_id(), &q.from.username).await;
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn register_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (role_type, user_type): (RoleType, UsrType),
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_name", "Message", msg, 
        "RoleType" => role_type,
        "UserType" => user_type
    );
    
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };

    match msg.text().map(ToOwned::to_owned) {
        Some(input_name) => {
            display_register_ops_name(&bot, dialogue.chat_id(), &user.username).await;
            log::debug!("Transitioning to RegisterOpsName with RoleType: {:?}, UsrType: {:?}, Name: {:?}", role_type, user_type, input_name);
            dialogue.update(State::RegisterOpsName {
                role_type,
                user_type,
                name: input_name
            }).await?
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, send me your full name, or type /cancel to abort."),
                &user.username,
            ).await;
            display_register_name(&bot, dialogue.chat_id(), &user.username).await;
        }
    }
    
    Ok(())
}

pub(super) async fn register_ops_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (role_type, user_type, name): (RoleType, UsrType, String),
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_ops_name", "Message", msg, 
        "RoleType" => role_type,
        "UserType" => user_type,
        "Name" => name
    );
    
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };

    match msg.text().map(ToOwned::to_owned) {
        Some(input_ops_name) => {
            send_msg(
                bot.send_message(
                    dialogue.chat_id(), 
                    format!(
                        "You are registering with the following details:\nNAME: {}\nOPS NAME: {}\nROLE: {}\nTYPE: {}",
                        name,
                        input_ops_name.to_uppercase(),
                        role_type.as_ref(),
                        user_type.as_ref()
                    )
                ),
                &user.username,
            ).await;
            display_register_confirmation(&bot, dialogue.chat_id(), &user.username).await;
            dialogue.update(State::RegisterComplete {
                role_type,
                user_type,
                name,
                ops_name: input_ops_name.to_uppercase()
            }).await?
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, send me your OPS NAME, or type /cancel to abort."),
                &user.username,
            ).await;
            display_register_name(&bot, dialogue.chat_id(), &user.username).await;
        }
    }

    Ok(())
}

pub(super) async fn register_complete(
    bot: Bot,
    dialogue: MyDialogue,
    (role_type, user_type, name, ops_name): (RoleType, UsrType, String, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_complete", "Callback", q, 
        "RoleType" => role_type,
        "UserType" => user_type,
        "Name" => name,
        "Ops Name" => ops_name
    );
    
    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;

            display_register_confirmation(&bot, dialogue.chat_id(), &q.from.username).await;
        }
        Some(confirmation) => {
            if confirmation == "YES" {
                // Add new user to the database
                match controllers::apply::apply_user(
                    &pool,
                    q.from.id.0,
                    q.from.full_name(),
                    name,
                    ops_name,
                    role_type,
                    user_type,
                )
                    .await {
                    Ok(true) => {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "Registered successfully, please wait for approval."),
                            &q.from.username,
                        ).await;
                        dialogue.update(State::Start).await?;
                    },
                    Ok(false) => {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "You have already applied, please wait for approval"),
                            &q.from.username,
                        ).await;
                        dialogue.update(State::Start).await?
                    },
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                }
            } else {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Cancelled registration."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            }
        }
    }

    Ok(())
}
