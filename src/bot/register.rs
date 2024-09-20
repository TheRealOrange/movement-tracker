use super::{handle_error, log_try_delete_msg, log_try_remove_markup, send_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::types::{RoleType, UsrType};
use crate::{controllers, log_endpoint_hit, notifier, utils};
use sqlx::PgPool;
use std::str::FromStr;
use strum::IntoEnumIterator;
use teloxide::payloads::EditMessageText;
use teloxide::prelude::*;
use teloxide::RequestError;
use teloxide::requests::JsonRequest;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ReplyParameters};

async fn display_role_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let roles = RoleType::iter()
        .map(|role| InlineKeyboardButton::callback(role.as_ref(), role.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Please select your role:")
            .reply_markup(InlineKeyboardMarkup::new([roles])),
        username
    ).await
}

async fn display_user_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let usrtypes = UsrType::iter()
        .map(|usrtype| InlineKeyboardButton::callback(usrtype.as_ref(), usrtype.as_ref()));

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

async fn display_register_confirmation(bot: &Bot, chat_id: ChatId, username: &Option<String>, name: &String, ops_name: &String, role_type: &RoleType, user_type: &UsrType) -> Option<MessageId> {
    let confirm = ["YES", "NO"]
        .map(|product| InlineKeyboardButton::callback(product, product));

    send_msg(
        bot.send_message(chat_id, format!(
            "You are registering with the following details:\nNAME: {}\nOPS NAME: {}\nROLE: {}\nTYPE: {}\n\nConfirm registration?",
            name,
            ops_name,
            role_type.as_ref(),
            user_type.as_ref()
        )).reply_markup(InlineKeyboardMarkup::new([confirm])),
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
            )
                .await;
            dialogue.update(State::Start).await?;
        }
        (Ok(false), Ok(true)) => {
            // User has a pending application
            send_msg(
                bot.send_message(dialogue.chat_id(), "You have an existing pending application. Please wait for approval.")
                    .reply_parameters(ReplyParameters::new(msg.id)),
                &user.username,
            )
                .await;
            dialogue.update(State::Start).await?;
        }
        (Ok(false), Ok(false)) => {
            // User is neither registered nor has a pending application, proceed with registration
            match display_role_types(&bot, dialogue.chat_id(), &user.username).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(msg_id) => {
                    log::debug!("Transitioning to RegisterRole");
                    dialogue.update(State::RegisterRole { msg_id }).await?;
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
    msg_id: MessageId,
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "register_role", "Callback", q,
        "MessageId" => msg_id
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(role) => {
            log::debug!("Received input: {:?}", &role);
            match RoleType::from_str(&role) {
                Ok(role_enum) => {
                    log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                    log::debug!("Selected role: {:?}", role_enum);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Selected role: {}", role_enum.as_ref())),
                        &q.from.username,
                    ).await;

                    match display_user_types(&bot, dialogue.chat_id(), &q.from.username).await {
                        None => {}
                        Some(new_msg_id) => {
                            log::debug!("Transitioning to RegisterType with RoleType: {:?}", role_enum);
                            dialogue.update(State::RegisterType { msg_id: new_msg_id, role_type: role_enum }).await?;
                        }
                    };
                }
                Err(e) => {
                    log::error!("Invalid role type received: {}", e);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                        &q.from.username,
                    ).await;
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn register_type(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, role_type): (MessageId, RoleType),
    q: CallbackQuery,
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_type", "Callback", q, 
        "MessageId" => msg_id,
        "RoleType" => role_type
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(usrtype) => {
            log::debug!("Received input: {:?}", &usrtype);
            match UsrType::from_str(&usrtype) {
                Ok(user_type_enum) => {
                    log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                    log::debug!("Selected user type: {:?}", user_type_enum);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Selected user type: {}", user_type_enum.as_ref())),
                        &q.from.username,
                    ).await;
                    match display_register_name(&bot, dialogue.chat_id(), &q.from.username).await {
                        None => {}
                        Some(new_msg_id) => {
                            log::debug!("Transitioning to RegisterName with RoleType: {:?}, UsrType: {:?}", role_type, user_type_enum);
                            dialogue.update(State::RegisterName { msg_id: new_msg_id, role_type, user_type: user_type_enum }).await?;
                        }
                    };
                }
                Err(e) => {
                    log::error!("Invalid user type received: {}", e);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                        &q.from.username,
                    ).await;
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
            let cleaned_name = utils::cleanup_name(&input_name_raw);

            // Validate that the name contains only allowed characters
            if !utils::is_valid_name(&cleaned_name) {
                // Invalid input: Notify the user and prompt to re-enter the name
                send_msg(
                    bot.send_message(
                        dialogue.chat_id(),
                        "Invalid name. Please use only letters and spaces. Try again or type /cancel to abort.",
                    ),
                    &user.username,
                ).await;
                // Remain in the current state to allow the user to re-enter their name
                return Ok(());
            }

            // Normalize the name (e.g., capitalize each word)
            let normalized_name = cleaned_name
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<String>>()
                .join(" ");

            if normalized_name.len() > utils::MAX_NAME_LENGTH {
                send_msg(
                    bot.send_message(
                        dialogue.chat_id(),
                        format!(
                            "OPS NAME is too long. Please enter a name with no more than {} characters.",
                            utils::MAX_NAME_LENGTH
                        ),
                    ),
                    &user.username,
                ).await;

                // Log the invalid attempt
                log::debug!(
                    "User {} entered OPS name exceeding max length: {}",
                    user.username.as_deref().unwrap_or("Unknown"),
                    normalized_name
                );

                return Ok(());
            }

            log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;

            match display_register_ops_name(&bot, dialogue.chat_id(), &user.username).await {
                None => {}
                Some(new_msg_id) => {
                    log::debug!(
                        "Transitioning to RegisterOpsName with RoleType: {:?}, UsrType: {:?}, Name: {}",
                        role_type,
                        user_type,
                        normalized_name
                    );
                    // Update the dialogue state to RegisterOpsName with the sanitized name
                    dialogue
                        .update(State::RegisterOpsName {
                            msg_id: new_msg_id,
                            role_type,
                            user_type,
                            name: normalized_name,
                        })
                        .await?
                }
            };
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
            let cleaned_ops_name = utils::cleanup_name(&input_ops_name_raw).to_uppercase();

            // Validate that the OPS name contains only allowed characters and is not empty
            if !utils::is_valid_ops_name(&cleaned_ops_name) {
                // Invalid input: Notify the user and prompt to re-enter OPS name
                send_msg(
                    bot.send_message(
                        dialogue.chat_id(),
                        "Invalid OPS NAME. Please use only letters and spaces. Try again or type /cancel to abort.",
                    ),
                    &user.username,
                ).await;
                // Log the invalid attempt
                log::debug!(
                    "User {} entered invalid OPS name: {}",
                    user.username.as_deref().unwrap_or("Unknown"),
                    input_ops_name_raw
                );
                // Remain in the current state to allow the user to re-enter OPS name
                return Ok(());
            }

            // Enforce a maximum length
            if cleaned_ops_name.len() > utils::MAX_NAME_LENGTH {
                send_msg(
                    bot.send_message(
                        dialogue.chat_id(),
                        format!(
                            "OPS NAME is too long. Please enter a name with no more than {} characters.",
                            utils::MAX_NAME_LENGTH
                        ),
                    ),
                    &user.username,
                ).await;

                // Log the invalid attempt
                log::debug!(
                    "User {} entered OPS name exceeding max length: {}",
                    user.username.as_deref().unwrap_or("Unknown"),
                    cleaned_ops_name
                );

                return Ok(());
            }

            // Check for OPS name uniqueness
            match controllers::user::user_exists_ops_name(&pool, &cleaned_ops_name).await {
                Ok(true) => {
                    // OPS name already exists: Notify the user and prompt to re-enter
                    send_msg(
                        bot.send_message( dialogue.chat_id(),
                            "OPS NAME already exists. Please choose a different OPS NAME or type /cancel to abort.", ),
                        &user.username,
                    ).await;
                    // Log the duplicate OPS name attempt
                    log::debug!(
                        "User {} attempted to use a duplicate OPS name: {}",
                        user.username.as_deref().unwrap_or("Unknown"),
                        cleaned_ops_name
                    );
                    // Remain in the current state to allow the user to re-enter OPS name
                    return Ok(());
                },
                Ok(false) => {
                    // OPS name is unique, proceed with registration
                    log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                    match display_register_confirmation(&bot, dialogue.chat_id(), &user.username, &name, &cleaned_ops_name, &role_type, &user_type).await {
                        None => {}
                        Some(new_msg_id) => {
                            dialogue.update(State::RegisterComplete {
                                msg_id: new_msg_id,
                                role_type,
                                user_type,
                                name,
                                ops_name: cleaned_ops_name
                            }).await?;
                        }
                    };
                },
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
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
    (msg_id, role_type, user_type, name, ops_name): (MessageId, RoleType, UsrType, String, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "register_complete", "Callback", q, 
        "MessageId" => msg_id,
        "RoleType" => role_type,
        "UserType" => user_type,
        "Name" => name,
        "Ops Name" => ops_name
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }
    
    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(confirmation) => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            if confirmation == "YES" {
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
                                "User @{} has applied:\nOPS NAME:{}\nNAME:{}",
                                &q.from.username.as_deref().unwrap_or("none"), ops_name, name
                            ).as_str(),
                            &pool,
                        ).await;

                        match bot.edit_message_text(dialogue.chat_id(), msg_id,
                            format!(
                                "Submitted registration with the following details:\nNAME: {}\nOPS NAME: {}\nROLE: {}\nTYPE: {}\n\nPlease wait for approval.?",
                                name,
                                ops_name,
                                role_type.as_ref(),
                                user_type.as_ref()
                            )
                        ).await {
                            Ok(_) => {}
                            Err(_) => {
                                log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
                                send_msg(
                                    bot.send_message(dialogue.chat_id(), format!(
                                        "Submitted registration with the following details:\nNAME: {}\nOPS NAME: {}\nROLE: {}\nTYPE: {}\n\nPlease wait for approval.?",
                                        name,
                                        ops_name,
                                        role_type.as_ref(),
                                        user_type.as_ref()
                                    )),
                                    &q.from.username,
                                ).await;
                            }
                        }
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
