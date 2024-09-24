use std::str::FromStr;

use sqlx::PgPool;
use sqlx::types::Uuid;

use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, ReplyParameters};

use super::{handle_error, log_try_delete_msg, log_try_remove_markup, match_callback_data, retrieve_callback_data, send_msg, send_or_edit_msg, validate_name, validate_ops_name, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::types::{RoleType, UsrType};
use crate::{controllers, log_endpoint_hit, notifier, utils};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumProperty;
use callback_data::{CallbackData, CallbackDataHandler};
use crate::utils::generate_prefix;

// Represents callback actions with optional associated data.
#[derive(Debug, Clone, Serialize, Deserialize, EnumProperty, CallbackData)]
pub enum RegisterCallbackData {
    // Selection actions for role and user type
    SelectRoleType { role_type: RoleType },
    SelectUserType { user_type: UsrType },
    
    // Confirmation actions
    ConfirmYes,
    ConfirmNo,
}

async fn display_role_types(bot: &Bot, chat_id: ChatId, username: &Option<String>, prefix: &String) -> Option<MessageId> {
    let roles = RoleType::iter()
        .map(|role_type| InlineKeyboardButton::callback(role_type.clone().as_ref(), RegisterCallbackData::SelectRoleType { role_type }.to_callback_data(prefix)));

    send_msg(
        bot.send_message(chat_id, "Please select your role:")
            .reply_markup(InlineKeyboardMarkup::new([roles])),
        username
    ).await
}

async fn display_user_types(bot: &Bot, chat_id: ChatId, username: &Option<String>, prefix: &String) -> Option<MessageId> {
    let usrtypes = UsrType::iter()
        .map(|user_type| InlineKeyboardButton::callback(user_type.clone().as_ref(), RegisterCallbackData::SelectUserType { user_type }.to_callback_data(prefix)));

    send_msg(
        bot.send_message(chat_id, "Please select your status:")
            .reply_markup(InlineKeyboardMarkup::new([usrtypes])),
       username
    ).await
}

async fn display_register_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    send_msg(
        bot.send_message(chat_id, "Type your full name:"),
        username,
    ).await
}

async fn display_register_ops_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    send_msg(
        bot.send_message(chat_id, "Type your OPS NAME:"),
        username,
    ).await
}

async fn display_register_confirmation(bot: &Bot, chat_id: ChatId, username: &Option<String>, name: &String, ops_name: &String, role_type: &RoleType, user_type: &UsrType, prefix: &String) -> Option<MessageId> {
    let confirm = [("YES", RegisterCallbackData::ConfirmYes), ("NO", RegisterCallbackData::ConfirmNo)]
        .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(prefix)));

    send_msg(
        bot.send_message(chat_id, format!(
            "You are registering with the following details:\nNAME: *{}*\nOPS NAME: `{}`\nROLE: `{}`\nTYPE: `{}`\n\nConfirm registration?",
            utils::escape_special_characters(&name),
            utils::escape_special_characters(&ops_name),
            role_type.as_ref(),
            user_type.as_ref()
        )).reply_markup(InlineKeyboardMarkup::new([confirm]))
            .parse_mode(ParseMode::MarkdownV2),
        username
    ).await
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

    let tele_id = user.id.0;

    // Check if the user is already registered or has a pending application
    match (
        controllers::user::user_exists_tele_id(&pool, tele_id).await,
        controllers::apply::user_has_pending_application(&pool, tele_id).await,
    ) {
        (Ok(true), _) => {
            // User is already registered
            send_msg(
                bot.send_message(dialogue.chat_id(), "You have already registered.")
                    .reply_parameters(ReplyParameters::new(msg.id)),
                &user.username,
            ).await;
            dialogue.update(State::Start).await?;
        }
        (Ok(false), Ok(true)) => {
            // User has a pending application
            send_msg(
                bot.send_message(dialogue.chat_id(), "You have an existing pending application. Please wait for approval.")
                    .reply_parameters(ReplyParameters::new(msg.id)),
                &user.username,
            ).await;
            dialogue.update(State::Start).await?;
        }
        (Ok(false), Ok(false)) => {
            // User is neither registered nor has a pending application, proceed with registration
            let prefix = generate_prefix();
            match display_role_types(&bot, dialogue.chat_id(), &user.username, &prefix).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(msg_id) => {
                    log::debug!("Transitioning to RegisterRole");
                    dialogue.update(State::RegisterRole { msg_id, prefix }).await?;
                }
            }
        }
        (_, _) => {
            // Handle unexpected errors during application check
            log::error!("Error checking application status for tele_id: {}", tele_id);
            handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
        }
    }

    Ok(())
}

pub(super) async fn register_role(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, prefix): (MessageId, String),
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "register_role", "Callback", q,
        "MessageId" => msg_id,
        "Prefix" => prefix
    );

    // Extract the callback data
    let data = match retrieve_callback_data(&bot, dialogue.chat_id(), &q).await {
        Ok(data) => data,
        Err(_) => { return Ok(()); }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    // Deserialize the callback data into the enum
    let callback = match match_callback_data(&bot, dialogue.chat_id(), &q.from.username, &data, &prefix).await {
        Ok(callback) => callback,
        Err(_) => { return Ok(()); }
    };
    
    match callback {
        RegisterCallbackData::SelectRoleType { role_type } => {
            log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
            log::debug!("Selected role: {:?}", role_type);
            send_msg(
                bot.send_message(dialogue.chat_id(), format!("Selected role: `{}`", role_type.as_ref())).parse_mode(ParseMode::MarkdownV2),
                &q.from.username,
            ).await;

            match display_user_types(&bot, dialogue.chat_id(), &q.from.username, &prefix).await {
                None => {}
                Some(new_msg_id) => {
                    log::debug!("Transitioning to RegisterType with RoleType: {:?}", role_type);
                    dialogue.update(State::RegisterType { msg_id: new_msg_id, prefix, role_type }).await?;
                }
            };
        }
        _ => {
            log::error!("Invalid role type received in chat ({})", dialogue.chat_id().0);
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn register_type(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, prefix, role_type): (MessageId, String, RoleType),
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_type", "Callback", q, 
        "MessageId" => msg_id,
        "Prefix" => prefix,
        "RoleType" => role_type
    );

    // Extract the callback data
    let data = match retrieve_callback_data(&bot, dialogue.chat_id(), &q).await {
        Ok(data) => data,
        Err(_) => { return Ok(()); }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    // Deserialize the callback data into the enum
    let callback = match match_callback_data(&bot, dialogue.chat_id(), &q.from.username, &data, &prefix).await {
        Ok(callback) => callback,
        Err(_) => { return Ok(()); }
    };

    match callback {
        RegisterCallbackData::SelectUserType { user_type } => {
            log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
            log::debug!("Selected user type: {:?}", user_type);
            send_msg(
                bot.send_message(dialogue.chat_id(), format!("Selected user type: `{}`", user_type.as_ref())).parse_mode(ParseMode::MarkdownV2),
                &q.from.username,
            ).await;
            match display_register_name(&bot, dialogue.chat_id(), &q.from.username).await {
                None => {}
                Some(new_msg_id) => {
                    log::debug!("Transitioning to RegisterName with RoleType: {:?}, UsrType: {:?}", role_type, user_type);
                    dialogue.update(State::RegisterName { msg_id: new_msg_id, role_type, user_type }).await?;
                }
            };
        }
        _ => {
            log::error!("Invalid user type received in chat ({})", dialogue.chat_id().0);
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn register_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (msg_id, role_type, user_type): (MessageId, RoleType, UsrType),
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_name", "Message", msg, 
        "MessageId" => msg_id,
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
        Some(input_name_raw) => {
            match validate_name(&bot, &dialogue, &user.username, input_name_raw, false).await {
                Ok(name) => {
                    log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                    match display_register_ops_name(&bot, dialogue.chat_id(), &user.username).await {
                        None => {}
                        Some(new_msg_id) => {
                            log::debug!(
                                "Transitioning to RegisterOpsName with RoleType: {:?}, UsrType: {:?}, Name: {}",
                                role_type,
                                user_type,
                                name
                            ); 
                            // Update the dialogue state to RegisterOpsName with the sanitized name
                            dialogue.update(State::RegisterOpsName {
                                    msg_id: new_msg_id,
                                    role_type,
                                    user_type,
                                    name,
                                }).await?
                        }
                    };
                }
                Err(_) => {
                    // Let the user retry
                    return Ok(());
                }
            }
        }
        None => {
            // If no text is found in the message, prompt the user to send their full name
            send_msg(
                bot.send_message(
                    dialogue.chat_id(),
                    "Please, send me your full name, or type /cancel to abort.",
                ),
                &user.username,
            ).await;
        }
    }
    
    Ok(())
}

pub(super) async fn register_ops_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (msg_id, role_type, user_type, name): (MessageId, RoleType, UsrType, String),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_ops_name", "Message", msg, 
        "MessageId" => msg_id,
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
        Some(input_ops_name_raw) => {
            match validate_ops_name(&bot, &dialogue, &user.username, input_ops_name_raw, &pool).await {
                Ok(ops_name) => {
                    // OPS name is unique, proceed with registration
                    let prefix = generate_prefix();
                    log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                    match display_register_confirmation(&bot, dialogue.chat_id(), &user.username, &name, &ops_name, &role_type, &user_type, &prefix).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => {
                            dialogue.update(State::RegisterComplete {
                                msg_id: new_msg_id, prefix,
                                role_type, user_type,
                                name, ops_name
                            }).await?;
                        }
                    };
                }
                Err(_) => {
                    // Let the user retry, or will auto transition to error state if database error occured
                    return Ok(());
                }
            }
        }
        None => {
            send_msg(
                bot.send_message(
                    dialogue.chat_id(),
                    "Please, send me your OPS NAME, or type /cancel to abort.",
                ),
                &user.username,
            ).await;
            display_register_ops_name(&bot, dialogue.chat_id(), &user.username).await;
        }
    }

    Ok(())
}

pub(super) async fn register_complete(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, prefix, role_type, user_type, name, ops_name): (MessageId, String, RoleType, UsrType, String, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_complete", "Callback", q, 
        "MessageId" => msg_id,
        "Prefix" => prefix,
        "RoleType" => role_type,
        "UserType" => user_type,
        "Name" => name,
        "Ops Name" => ops_name
    );

    // Extract the callback data
    let data = match retrieve_callback_data(&bot, dialogue.chat_id(), &q).await {
        Ok(data) => data,
        Err(_) => { return Ok(()); }
    };

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    // Deserialize the callback data into the enum
    let callback = match match_callback_data(&bot, dialogue.chat_id(), &q.from.username, &data, &prefix).await {
        Ok(callback) => callback,
        Err(_) => { return Ok(()); }
    };
    
    match callback {
        RegisterCallbackData::ConfirmYes => {
            // Add new user to the database
            match controllers::apply::apply_user(
                &pool,
                q.from.id.0,
                q.from.full_name(),
                name.clone(),
                ops_name.clone(),
                role_type.clone(),
                user_type.clone(),
            )
                .await {
                Ok(true) => {
                    notifier::emit::register_notifications(
                        &bot,
                        format!(
                            "User {} has applied:\nOPS NAME: `{}`\nNAME: *{}*",
                            utils::username_link_tag(&q.from),
                            utils::escape_special_characters(&ops_name), utils::escape_special_characters(&name)
                        ).as_str(),
                        &pool,
                    ).await;

                    let registration_text_str = format!(
                        "Submitted registration with the following details:\nROLE: `{}`\nTYPE: `{}`\nNAME: *{}*\nOPS NAME: `{}`\n\nPlease wait for approval.",
                        role_type.as_ref(),
                        user_type.as_ref(),
                        utils::escape_special_characters(&name), utils::escape_special_characters(&ops_name)
                    );

                    // Send or edit message
                    send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), registration_text_str, None, Some(ParseMode::MarkdownV2)).await;
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
        }
        RegisterCallbackData::ConfirmNo => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Cancelled registration."),
                &q.from.username,
            ).await;
            dialogue.update(State::Start).await?
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}
