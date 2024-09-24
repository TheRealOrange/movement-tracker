use std::str::FromStr;
use chrono::Local;

use sqlx::PgPool;

use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, User};

use super::{handle_error, log_try_delete_msg, log_try_remove_markup, match_callback_data, retrieve_callback_data, send_msg, send_or_edit_msg, validate_name, validate_ops_name, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::types::{RoleType, UserInfo, Usr, UsrType};
use crate::{controllers, log_endpoint_hit, notifier, utils};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumProperty;
use uuid::Uuid;
use callback_data::{CallbackData, CallbackDataHandler};
use crate::utils::generate_prefix;

// Represents callback actions with optional associated data.
#[derive(Debug, Clone, Serialize, Deserialize, EnumProperty, CallbackData)]
enum UserEditCallbacks {
    // Completion Actions
    Done,
    Delete,
    Cancel,

    // Edit field Actions
    Name,
    OpsName,
    RoleType,
    UserType,
    Admin,

    // Role and User type selection Actions
    SelectRoleType { role_type: RoleType },
    SelectUserType { user_type: UsrType },

    // Select Admin Yes/No Actions
    AdminYes,
    AdminNo,

    // Delete confirmation Actions
    DeleteYes,
    DeleteNo
}

fn get_inline_keyboard(is_last_admin: bool, prefix: &String) -> InlineKeyboardMarkup {
    let mut options: Vec<Vec<InlineKeyboardButton>> = [
        ("NAME", UserEditCallbacks::Name),
        ("OPS NAME", UserEditCallbacks::OpsName),
        ("ROLE", UserEditCallbacks::RoleType),
        ("TYPE", UserEditCallbacks::UserType)
    ]
        .into_iter()
        .map(|(text, data)| vec![InlineKeyboardButton::callback(text, data.to_callback_data(&prefix))])
        .collect();

    // Conditionally add the "ADMIN" button
    if !is_last_admin {
        options.push(vec![InlineKeyboardButton::callback("ADMIN", UserEditCallbacks::Admin.to_callback_data(&prefix))]);
    }

    let mut confirm_row: Vec<InlineKeyboardButton> = [("DONE", UserEditCallbacks::Done), ("CANCEL", UserEditCallbacks::Cancel)]
        .into_iter()
        .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(&prefix)))
        .collect();

    if !is_last_admin {
        confirm_row.push(InlineKeyboardButton::callback("DELETE", UserEditCallbacks::Delete.to_callback_data(&prefix)));
    }

    // Add the confirmation row to the options
    options.push(confirm_row);

    // Construct and return the InlineKeyboardMarkup
    InlineKeyboardMarkup::new(options)
}

fn get_user_edit_text(user_details: &Usr) -> String {
    format!("Details of user:\nNAME: *{}*\nOPS NAME: `{}`\nROLE: `{}`\nTYPE: `{}`\nIS ADMIN: *{}*\nADDED: _{}_\nUPDATED: _{}_\n\nEdited: _{}_\nDo you wish to edit any entries?",
        format!("[{}](tg://user?id={})", utils::escape_special_characters(&user_details.name), user_details.tele_id as u64),
        &user_details.ops_name,
        user_details.role_type.as_ref(),
        user_details.usr_type.as_ref(),
        if user_details.admin == true { "YES" } else { "NO" }, 
        utils::escape_special_characters(&user_details.updated.with_timezone(&Local).format("%b-%d-%Y %H:%M:%S").to_string()),
        utils::escape_special_characters(&user_details.updated.with_timezone(&Local).format("%b-%d-%Y %H:%M:%S").to_string()),
        utils::escape_special_characters(&Local::now().format("%d%m %H%M.%S").to_string())
    )
}

async fn display_user_edit_prompt(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    user_details: &Usr,
    is_last_admin: bool,
    prefix: &String,
    msg_id: Option<MessageId>
) -> Option<MessageId> {
    send_or_edit_msg(&bot, chat_id, username, msg_id, get_user_edit_text(user_details), Some(get_inline_keyboard(is_last_admin, prefix)), Some(ParseMode::MarkdownV2)).await
}

async fn display_edit_user_types(bot: &Bot, chat_id: ChatId, username: &Option<String>, prefix: &String) -> Option<MessageId> {
    let usrtypes = UsrType::iter()
        .map(|usrtype| InlineKeyboardButton::callback(usrtype.clone().as_ref(), UserEditCallbacks::SelectUserType { user_type: usrtype }.to_callback_data(&prefix)));

    send_msg(
        bot.send_message(chat_id, "Select user type:")
            .reply_markup(InlineKeyboardMarkup::new([usrtypes])),
        username
    ).await
}

async fn display_edit_role_types(bot: &Bot, chat_id: ChatId, username: &Option<String>, prefix: &String) -> Option<MessageId> {
    let role_types = RoleType::iter()
        .map(|roletype| InlineKeyboardButton::callback(roletype.clone().as_ref(), UserEditCallbacks::SelectRoleType { role_type: roletype }.to_callback_data(&prefix)));

    send_msg(
        bot.send_message(chat_id, "Select role type:")
            .reply_markup(InlineKeyboardMarkup::new([role_types])),
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

async fn display_edit_admin(bot: &Bot, chat_id: ChatId, username: &Option<String>, prefix: &String) -> Option<MessageId> {
    let confirm = [("YES", UserEditCallbacks::AdminYes), ("NO", UserEditCallbacks::AdminNo)]
        .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(&prefix)));

    send_msg(
        bot.send_message(chat_id, "Make user admin?")
            .reply_markup(InlineKeyboardMarkup::new([confirm])),
        username
    ).await
}

async fn display_delete_confirmation(bot: &Bot, chat_id: ChatId, username: &Option<String>, prefix: &String) -> Option<MessageId> {
    let confirm = [("YES", UserEditCallbacks::DeleteYes), ("NO", UserEditCallbacks::DeleteNo)]
        .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(&prefix)));

    send_msg(
        bot.send_message(chat_id, "Delete user? (they will have to re-register)")
            .reply_markup(InlineKeyboardMarkup::new([confirm])),
        username
    ).await
}

async fn handle_go_back(
    bot: &Bot, 
    dialogue: &MyDialogue, 
    username: &Option<String>,
    user_details: Usr,
    pool: &PgPool,
    prefix: String,
    msg_id: MessageId
) -> HandlerResult {
    let is_last_admin = match controllers::user::is_last_admin(&pool, user_details.id).await {
        Ok(is_last_admin) => is_last_admin,
        Err(_) => {
            handle_error(&bot, &dialogue, dialogue.chat_id(), username).await;
            return Ok(());
        }
    };
    match display_user_edit_prompt(&bot, dialogue.chat_id(), username, &user_details, is_last_admin, &prefix, Some(msg_id)).await {
        None => dialogue.update(State::ErrorState).await?,
        Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details, prefix }).await?
    }
    
    Ok(())
}

async fn display_enter_ops_name(bot: &Bot, dialogue: &MyDialogue, username: &Option<String>, result: Vec<UserInfo>) -> Option<MessageId> {
    // Calculate the length of the longest `ops_name`
    let max_len = result.iter()
        .map(|info| info.ops_name.len())
        .max()
        .unwrap_or(0); // Handle case when result is empty

    // Use `tokio::spawn` to concurrently fetch usernames and format the list
    let mut tasks = vec![];

    for info in result {
        let bot_clone = bot.clone();
        tasks.push(tokio::spawn(async move {
            // Log the ops_name and name before formatting
            log::debug!("Formatting user: ops_name: '{}', name: '{}', tele_id: {}", info.ops_name, info.name, info.tele_id);

            // Dynamically fetch the username using getChat
            let mention = match bot_clone.get_chat(ChatId(info.tele_id)).await {
                Ok(chat) => {
                    if let Some(username) = chat.username() {
                        format!("@{}", utils::escape_special_characters(&username)) // Bold and italic username
                    } else {
                        format!("__[{}](tg://user?id={})__", utils::escape_special_characters(&info.name), info.tele_id as u64) // Fallback to link by tele_id
                    }
                },
                Err(e) => {
                    log::error!("Error fetching chat info for user {}: {}", info.tele_id, e);
                    format!("__[{}](tg://user?id={})__", utils::escape_special_characters(&info.name), info.tele_id as u64) // Fallback to link if error occurs
                }
            };

            // Format each line, ensuring `ops_name` has fixed width based on the longest one
            format!(
                "\\- *`{:<width$}`* \\- {}", // Format the ops_name with fixed width
                utils::escape_special_characters(&info.ops_name), // Escape special characters in ops_name
                mention,
                width = max_len // Dynamically set the width
            )
        }));
    }

    // Collect all the tasks and wait for them to complete
    let mut formatted_list = Vec::with_capacity(tasks.len());
    for task in tasks {
        match task.await {
            Ok(result) => formatted_list.push(result),
            Err(e) => log::error!("Error in task: {}", e),
        }
    }

    let formatted_list = formatted_list.join("\n");

    log::debug!("Formatted text {}", formatted_list);

    // Construct the message
    let message = format!(
        "List of users:\n{}\n\nPlease enter the `OPS NAME` of the user you wish to edit\\.",
        formatted_list
    );

    // Send the message to the user
    send_msg(
        bot.send_message(dialogue.chat_id(), message).parse_mode(ParseMode::MarkdownV2),
        username
    ).await
}

async fn handle_show_prompt(bot: &Bot, dialogue: &MyDialogue, pool: &PgPool, user: &User) -> HandlerResult {
    if let Ok(result) = controllers::user::get_all_user_info(pool).await {
        display_enter_ops_name(&bot, &dialogue, &user.username, result).await;
        dialogue.update(State::UserSelect).await?;
    } else {
        handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
    }

    Ok(())
}

async fn display_retry_user_select(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    // Send the message to the user
    send_msg(
        bot.send_message(chat_id, "Please enter the `OPS NAME` of the user you wish to edit:").parse_mode(ParseMode::MarkdownV2),
        username
    ).await
}
async fn handle_ops_name_input(bot: &Bot, dialogue: &MyDialogue, pool: &PgPool, user: &User, ops_name: String, show_users_on_err: bool) -> HandlerResult {
    let cleaned_ops_name = ops_name.trim().to_uppercase();

    // Get the user in the database
    match controllers::user::user_exists_ops_name(&pool, cleaned_ops_name.as_ref()).await{
        Ok(exists) => {
            if exists {
                match controllers::user::get_user_by_ops_name(&pool, cleaned_ops_name.as_ref()).await {
                    Ok(user_details) => {
                        // Generate random prefix to make the IDs only applicable to this dialogue instance
                        let prefix = generate_prefix();

                        let is_last_admin = match controllers::user::is_last_admin(&pool, user_details.id).await {
                            Ok(is_last_admin) => is_last_admin,
                            Err(_) => {
                                handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
                                return Ok(());
                            }
                        };
                        match display_user_edit_prompt(&bot, dialogue.chat_id(), &user.username, &user_details, is_last_admin, &prefix, None).await {
                            None => dialogue.update(State::ErrorState).await?,
                            Some(msg_id) => dialogue.update(State::UserEdit { msg_id, user_details, prefix }).await?
                        };
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                }
            } else {
                if show_users_on_err {
                    handle_show_prompt(&bot, &dialogue,&pool, &user).await?;
                } else {
                    // Send the message to the user
                    display_retry_user_select(&bot, dialogue.chat_id(), &user.username).await;
                    dialogue.update(State::UserSelect).await?;
                }
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }
    
    Ok(())
}

pub(super) async fn user(
    bot: Bot, 
    dialogue: MyDialogue, 
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
    
    if !ops_name.is_empty() {
        handle_ops_name_input(&bot, &dialogue, &pool, &user, ops_name, true).await?;
    } else {
        handle_show_prompt(&bot, &dialogue,&pool, &user).await?;
    }
    
    Ok(())
}

pub(super) async fn user_select(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "user_select", "Message", msg);

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    match msg.text().map(ToOwned::to_owned) {
        Some(ops_name) => {
            handle_ops_name_input(&bot, &dialogue, &pool, &user, ops_name, false).await?;
        }
        None => {
            display_retry_user_select(&bot, dialogue.chat_id(), &user.username).await;
        }
    }

    Ok(())
}

pub(super) async fn user_edit_prompt(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, user_details, prefix): (MessageId, Usr, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_prompt", "Callback", q,
        "MessageId" => msg_id,
        "User Details" => user_details,
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
        UserEditCallbacks::Done => {
            log_try_delete_msg(&bot, dialogue.chat_id(), msg_id).await;
            let original_user = match controllers::user::get_user_by_uuid(&pool, user_details.id).await {
                Ok(user) => user,
                Err(_) => {
                    handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await;
                    log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                    return Ok(());
                }
            };
            match controllers::user::update_user(
                &pool,
                &user_details
            ).await {
                Ok(user_updated) => {
                    // Build a list of changes
                    let mut changes = Vec::new();

                    if original_user.name != user_updated.name {
                        changes.push(format!(
                            "*Name:* {} ➡️ {}",
                            original_user.name,
                            user_updated.name
                        ));
                    }

                    if original_user.ops_name != user_updated.ops_name {
                        changes.push(format!(
                            "*OPS Name:* `{}` ➡️ `{}`",
                            original_user.ops_name,
                            user_updated.ops_name
                        ));
                    }

                    if original_user.role_type != user_updated.role_type {
                        changes.push(format!(
                            "*Role:* `{:?}` ➡️ `{:?}`",
                            original_user.role_type,
                            user_updated.role_type
                        ));
                    }

                    if original_user.usr_type != user_updated.usr_type {
                        changes.push(format!(
                            "*Type:* `{:?}` ➡️ `{:?}`",
                            original_user.usr_type,
                            user_updated.usr_type
                        ));
                    }

                    if original_user.admin != user_updated.admin {
                        changes.push(format!(
                            "*Admin Status:* {} ➡️ {}",
                            if original_user.admin { "YES" } else { "NO" },
                            if user_updated.admin { "YES" } else { "NO" }
                        ));
                    }

                    // Combine all changes into a single message
                    let changes_message = if changes.is_empty() {
                        "No changes were made\\.".to_string()
                    } else {
                        changes.join("\n")
                    };

                    if !changes.is_empty() {
                        // Emit system notification with the changes
                        notifier::emit::system_notifications(
                            &bot,
                            &format!(
                                "{} has amended user details for *{}*:\n{}",
                                utils::username_link_tag(&q.from),
                                utils::escape_special_characters(&user_details.ops_name),
                                changes_message
                            ),
                            &pool,
                            q.from.id.0 as i64
                        ).await;
                    }

                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!(
                            "Updated user details:\n{}\nADDED: _{}_\nUPDATED: _{}_",
                            changes_message,
                            utils::escape_special_characters(&user_updated.updated.format("%b-%d-%Y %H:%M:%S").to_string()),
                            utils::escape_special_characters(&user_updated.updated.format("%b-%d-%Y %H:%M:%S").to_string())
                        )).parse_mode(ParseMode::MarkdownV2),
                        &q.from.username,
                    ).await;

                    // Inform users if their admin status changed
                    if user_updated.admin != original_user.admin {
                        send_msg(
                            bot.send_message(ChatId(user_updated.tele_id),
                                             format!("{} Use /help to see available actions.",
                                                     if user_updated.admin { "You are now an admin. Use /notify to configure notifications." } else { "You are no longer an admin." }
                                             ), ),
                            &q.from.username,
                        ).await;
                    }

                    dialogue.update(State::Start).await?;
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
            }
        }
        UserEditCallbacks::Delete => {
            match display_delete_confirmation(&bot, dialogue.chat_id(), &q.from.username, &prefix).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(change_msg_id) => dialogue.update(State::UserEditDeleteConfirm { msg_id, change_msg_id, user_details, prefix }).await?
            };
        }
        UserEditCallbacks::Cancel => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            send_msg(
                bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                &q.from.username,
            ).await;
            dialogue.update(State::Start).await?
        }
        UserEditCallbacks::Name => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            match display_edit_name(&bot, dialogue.chat_id(), &q.from.username).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(change_msg_id) => dialogue.update(State::UserEditName { msg_id, change_msg_id, user_details, prefix }).await?
            };
        }
        UserEditCallbacks::OpsName => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            match display_edit_ops_name(&bot, dialogue.chat_id(), &q.from.username).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(change_msg_id) => dialogue.update(State::UserEditOpsName { msg_id, change_msg_id, user_details, prefix }).await?
            };
        }
        UserEditCallbacks::RoleType => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            match display_edit_role_types(&bot, dialogue.chat_id(), &q.from.username, &prefix).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(change_msg_id) => dialogue.update(State::UserEditRole { msg_id, change_msg_id, user_details, prefix }).await?
            };
        }
        UserEditCallbacks::UserType => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            match display_edit_user_types(&bot, dialogue.chat_id(), &q.from.username, &prefix).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(change_msg_id) => dialogue.update(State::UserEditType { msg_id, change_msg_id, user_details, prefix }).await?
            };
        }
        UserEditCallbacks::Admin => {
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            match display_edit_admin(&bot, dialogue.chat_id(), &q.from.username, &prefix).await {
                None => dialogue.update(State::ErrorState).await?,
                Some(change_msg_id) => dialogue.update(State::UserEditAdmin { msg_id, change_msg_id, user_details, prefix }).await?
            };
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option. Type /cancel to abort."),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn user_edit_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (msg_id, change_msg_id, mut user_details, prefix): (MessageId, MessageId, Usr, String),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_name", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details,
        "Prefix" => prefix
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
                    handle_go_back(&bot, &dialogue, &user.username, user_details, &pool, prefix, msg_id).await?;
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
    (msg_id, change_msg_id, mut user_details, prefix): (MessageId, MessageId, Usr, String),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_ops_name", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details,
        "Prefix" => prefix
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
                    handle_go_back(&bot, &dialogue, &user.username, user_details, &pool, prefix, msg_id).await?;
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

pub(super) async fn user_edit_role(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, mut user_details, prefix): (MessageId, MessageId, Usr, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_role", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details,
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
        UserEditCallbacks::SelectRoleType { role_type: role_type_enum } => {
            log::debug!("Selected role type: {:?}", role_type_enum);
            send_msg(
                bot.send_message(dialogue.chat_id(), format!("Selected role type: `{}`", utils::escape_special_characters(&role_type_enum.as_ref())))
                    .parse_mode(ParseMode::MarkdownV2),
                &q.from.username,
            ).await;

            user_details.role_type = role_type_enum;
            log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
            handle_go_back(&bot, &dialogue, &q.from.username, user_details, &pool, prefix, msg_id).await?;
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option. Type /cancel to abort."),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn user_edit_type(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, mut user_details, prefix): (MessageId, MessageId, Usr, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_type", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details,
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
        UserEditCallbacks::SelectUserType { user_type: user_type_enum } => {
            log::debug!("Selected user type: {:?}", user_type_enum);
            send_msg(
                bot.send_message(dialogue.chat_id(), format!("Selected user type: `{}`", utils::escape_special_characters(&user_type_enum.as_ref())))
                    .parse_mode(ParseMode::MarkdownV2),
                &q.from.username,
            ).await;

            user_details.usr_type = user_type_enum;
            log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
            handle_go_back(&bot, &dialogue, &q.from.username, user_details, &pool, prefix, msg_id).await?;
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option. Type /cancel to abort."),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}

pub(super) async fn user_edit_admin(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, mut user_details, prefix): (MessageId, MessageId, Usr, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_admin", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details,
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
        UserEditCallbacks::AdminYes => {
            user_details.admin = true;
        }
        UserEditCallbacks::AdminNo => {
            user_details.admin = false;
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option. Type /cancel to abort."),
                &q.from.username,
            ).await;
            return Ok(());
        }
    }
    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
    handle_go_back(&bot, &dialogue, &q.from.username, user_details, &pool, prefix, msg_id).await?;

    Ok(())
}

pub(super) async fn user_edit_delete(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, user_details, prefix): (MessageId, MessageId, Usr, String),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "user_edit_delete", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "User Details" => user_details,
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
        UserEditCallbacks::DeleteYes => {
            match controllers::user::remove_user_by_uuid(&pool, user_details.id).await {
                Ok(success) => {
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                    if success {
                        // Emit system notification to indicate who has deleted the user
                        notifier::emit::system_notifications(
                            &bot,
                            format!(
                                "{} has deleted the user:\nOPS NAME: `{}`\nNAME: *{}*",
                                utils::username_link_tag(&q.from),
                                utils::escape_special_characters(&user_details.ops_name),
                                format!("[{}](tg://user?id={})", utils::escape_special_characters(&user_details.name), user_details.tele_id as u64)
                            ).as_str(),
                            &pool,
                            q.from.id.0 as i64
                        ).await;

                        send_msg(
                            bot.send_message(ChatId(user_details.tele_id), "You have been deregistered."),
                            &q.from.username,
                        ).await;

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
        }
        UserEditCallbacks::DeleteNo => {
            log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
            handle_go_back(&bot, &dialogue, &q.from.username, user_details, &pool, prefix, msg_id).await?;
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option. Type /cancel to abort."),
                &q.from.username,
            ).await;
            return Ok(());
        }
    }
    
    Ok(())
}