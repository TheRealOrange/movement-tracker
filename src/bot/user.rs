use super::{handle_error, log_try_delete_msg, log_try_remove_markup, send_msg, validate_name, validate_ops_name, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::types::{Usr, UsrType};
use crate::{controllers, log_endpoint_hit};
use sqlx::PgPool;
use std::str::FromStr;
use chrono::Local;
use strum::IntoEnumIterator;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId};

fn get_inline_keyboard() -> InlineKeyboardMarkup {
    let mut options: Vec<Vec<InlineKeyboardButton>> = ["NAME", "OPS NAME", "TYPE", "ADMIN"]
        .into_iter()
        .map(|option| vec![InlineKeyboardButton::callback(option, option)])
        .collect();
    let confirm = ["DONE", "DELETE", "CANCEL"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    options.push(confirm);

    InlineKeyboardMarkup::new(options)
}

fn get_user_edit_text(user_details: &Usr) -> String {
    format!("Details of user:\nNAME: {}\nOPS NAME: {}\nROLE: {}\nTYPE: {}\nIS ADMIN: {}\nADDED: {}\nUPDATED: {}\n\nEdited:{}\nDo you wish to edit any entries?",
        &user_details.name,
        &user_details.ops_name,
        user_details.role_type.as_ref(),
        user_details.usr_type.as_ref(),
        if user_details.admin == true { "YES" } else { "NO" },
        &user_details.updated.with_timezone(&Local).format("%b-%d-%Y %H:%M:%S").to_string(),
        &user_details.updated.with_timezone(&Local).format("%b-%d-%Y %H:%M:%S").to_string(),
        Local::now().format("%d%m %H%M.%S").to_string()
    )
}

async fn display_user_edit_prompt(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    user_details: &Usr,
    msg_id: Option<MessageId>
) -> Option<MessageId> {
    match msg_id {
        None => {
            // Send a new message
            send_msg(
                bot.send_message(
                    chat_id,
                    get_user_edit_text(user_details)
                ).reply_markup(get_inline_keyboard()),
                username
            ).await
        }
        Some(msg_id) => {
            // Edit message rather than sending
            match bot.edit_message_text(chat_id, msg_id, get_user_edit_text(user_details))
                .reply_markup(get_inline_keyboard())
                .await {
                Ok(edited) => Some(edited.id),
                Err(_) => {
                    // Failed to edit, send a new message
                    log_try_delete_msg(&bot, chat_id, msg_id).await;
                    send_msg(
                        bot.send_message(
                            chat_id,
                            get_user_edit_text(user_details)
                        ).reply_markup(get_inline_keyboard()),
                        username
                    ).await
                }
            }
        }
    }
}

async fn display_edit_user_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let usrtypes = UsrType::iter()
        .map(|usrtype| InlineKeyboardButton::callback(usrtype.as_ref(), usrtype.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Select user type:")
            .reply_markup(InlineKeyboardMarkup::new([usrtypes])),
        username
    ).await
}

async fn display_edit_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    send_msg(
        bot.send_message(chat_id, "Enter full name:"),
        username,
    ).await
}

async fn display_edit_ops_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    send_msg(
        bot.send_message(chat_id, "Enter OPS NAME:"),
        username,
    ).await
}

async fn display_edit_admin(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let confirm = ["YES", "NO"]
        .map(|product| InlineKeyboardButton::callback(product, product));

    send_msg(
        bot.send_message(chat_id, "Make user admin?")
            .reply_markup(InlineKeyboardMarkup::new([confirm])),
        username
    ).await
}

async fn display_delete_confirmation(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let confirm = ["YES", "NO"]
        .map(|product| InlineKeyboardButton::callback(product, product));

    send_msg(
        bot.send_message(chat_id, "Delete user? (they will have to re-register)")
            .reply_markup(InlineKeyboardMarkup::new([confirm])),
        username
    ).await
}

pub(super) async fn user(
    bot: Bot, 
    dialogue: 
    MyDialogue, 
    msg: Message,
    ops_name: String,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "user", "Command", msg);

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Get the user in the database
    match controllers::user::user_exists_ops_name(&pool, ops_name.as_ref()).await{
        Ok(exists) => {
            if exists {
                match controllers::user::get_user_by_ops_name(&pool, ops_name.as_ref()).await {
                    Ok(user_details) => {
                        match display_user_edit_prompt(&bot, dialogue.chat_id(), &user.username, &user_details, None).await {
                            None => dialogue.update(State::ErrorState).await?,
                            Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details }).await?
                        };
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                }
            } else {
                // Display a list of valid OPS names and prompt the user to use a correct one
                if let Ok(result) = controllers::user::get_all_ops_names(&pool).await {
                    let formatted_list = result.join("\n");

                    // Send the list to the user
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Valid OPS Names:\n{}\n\nNo such user. Please use the command /user <OPS NAME> with one of the valid OPS names listed above.", formatted_list)),
                        &user.username
                    ).await;
                } else {
                    handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                }
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }
    
    Ok(())
}

pub(super) async fn user_edit_prompt(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, user_details): (MessageId, Usr),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_prompt", "Callback", q,
        "MessageId" => msg_id,
        "User Details" => user_details
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
        Some(option) => {
            match option.as_str() {
                "DONE" => {
                    match controllers::user::update_user(
                        &pool,
                        &user_details
                    ).await {
                        Ok(user_updated) => {
                            send_msg(
                                bot.send_message(dialogue.chat_id(), format!(
                                    "Updates user details\nNAME: {}\nOPS NAME: {}\nROLE: {}\nTYPE: {}\nIS ADMIN: {}\nADDED: {}\nUPDATED: {}\n",
                                    user_updated.name,
                                    user_updated.ops_name,
                                    user_updated.role_type.as_ref(),
                                    user_updated.usr_type.as_ref(),
                                    if user_details.admin == true { "YES" } else { "NO" },
                                    user_updated.updated.format("%b-%d-%Y %H:%M:%S").to_string(),
                                    user_updated.updated.format("%b-%d-%Y %H:%M:%S").to_string()
                                )),
                                &q.from.username,
                            ).await;

                            dialogue.update(State::Start).await?;
                        }
                        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                    }
                }
                "DELETE" => {
                    match display_delete_confirmation(&bot, dialogue.chat_id(), &q.from.username).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(change_msg_id) => dialogue.update(State::UserEditDeleteConfirm { msg_id, change_msg_id, user_details }).await?
                    };
                }
                "CANCEL" => {
                    log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                        &q.from.username,
                    ).await;
                    dialogue.update(State::Start).await?
                }
                "NAME" => {
                    match display_edit_name(&bot, dialogue.chat_id(), &q.from.username).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(change_msg_id) => dialogue.update(State::UserEditName { msg_id, change_msg_id, user_details }).await?
                    };
                }
                "OPS NAME" => {
                    match display_edit_ops_name(&bot, dialogue.chat_id(), &q.from.username).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(change_msg_id) => dialogue.update(State::UserEditOpsName { msg_id, change_msg_id, user_details }).await?
                    };
                }
                "TYPE" => {
                    match display_edit_user_types(&bot, dialogue.chat_id(), &q.from.username).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(change_msg_id) => dialogue.update(State::UserEditType { msg_id, change_msg_id, user_details }).await?
                    };
                }
                "ADMIN" => {
                    match display_edit_admin(&bot, dialogue.chat_id(), &q.from.username).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(change_msg_id) => dialogue.update(State::UserEditAdmin { msg_id, change_msg_id, user_details }).await?
                    };
                }
                _ => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Invalid option."),
                        &q.from.username,
                    ).await;
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn user_edit_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (msg_id, change_msg_id, mut user_details): (MessageId, MessageId, Usr),
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_name", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details
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
                    user_details.name = name.clone();
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Name updated to: {}", name)),
                        &user.username,
                    ).await;
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    match display_user_edit_prompt(&bot, dialogue.chat_id(), &user.username, &user_details, Some(msg_id)).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details }).await?
                    }
                }
                Err(_) => {
                    // Let the user retry
                    return Ok(());
                }
            }

        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter a full name, or type /cancel to abort."),
                &user.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn user_edit_ops_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (msg_id, change_msg_id, mut user_details): (MessageId, MessageId, Usr),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_ops_name", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details
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
                    user_details.ops_name = ops_name.clone();
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Name updated to: {}", ops_name)),
                        &user.username,
                    ).await;
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    match display_user_edit_prompt(&bot, dialogue.chat_id(), &user.username, &user_details, Some(msg_id)).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details }).await?
                    };
                }
                Err(_) => {
                    // Let the user retry
                    return Ok(());
                }
            }
        }
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Please, enter a OPS NAME, or type /cancel to abort."),
                &user.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn user_edit_type(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, mut user_details): (MessageId, MessageId, Usr),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_type", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details
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
                    log::debug!("Selected user type: {:?}", user_type_enum);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Selected user type: {}", user_type_enum.as_ref())),
                        &q.from.username,
                    ).await;

                    user_details.usr_type = user_type_enum;
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    match display_user_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &user_details, Some(msg_id)).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details }).await?
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

pub(super) async fn user_edit_admin(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, mut user_details): (MessageId, MessageId, Usr),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_admin", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details
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
        Some(make_admin_input) => {
            log::debug!("Received input: {:?}", &make_admin_input);
            if make_admin_input == "YES" {
                user_details.admin = true;
            } else if make_admin_input == "NO" {
                user_details.admin = false;
            } else {
                log::error!("Invalid set admin input received: {}", make_admin_input);
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                    &q.from.username,
                ).await;
                return Ok(())
            }

            log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
            match display_user_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &user_details, Some(msg_id)).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details }).await?
            };
        }
    }

    Ok(())
}

pub(super) async fn user_edit_delete(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, user_details): (MessageId, MessageId, Usr),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_delete", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details
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

            display_delete_confirmation(&bot, dialogue.chat_id(), &q.from.username).await;
        }
        Some(make_admin_input) => {
            log::debug!("Received input: {:?}", &make_admin_input);
            if make_admin_input == "YES" {
                match controllers::user::remove_user_by_uuid(&pool, user_details.id).await {
                    Ok(success) => {
                        if success {
                            send_msg(
                                bot.send_message(dialogue.chat_id(), format!("Successfully removed user: {}", user_details.ops_name)),
                                &q.from.username
                            ).await;
                        } else {
                            send_msg(
                                bot.send_message(dialogue.chat_id(), format!("No such user: {}", user_details.ops_name)),
                                &q.from.username
                            ).await;
                        }

                        dialogue.update(State::Start).await?;
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                }
            } else if make_admin_input == "NO" {
                log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                match display_user_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &user_details, Some(msg_id)).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details }).await?
                };
            } else {
                log::error!("Invalid delete confirmation input received: {}", make_admin_input);
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                    &q.from.username,
                ).await;
                display_delete_confirmation(&bot, dialogue.chat_id(), &q.from.username).await;
            }
        }
    }

    Ok(())
}