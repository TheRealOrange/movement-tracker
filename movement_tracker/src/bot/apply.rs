use crate::bot::state::State;
use crate::bot::{handle_error, log_try_delete_msg, log_try_remove_markup, send_msg, send_or_edit_msg, validate_name, validate_ops_name, HandlerResult, MyDialogue};
use crate::types::{Apply, RoleType, UsrType};
use crate::{controllers, log_endpoint_hit, notifier, utils};
use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::types::Uuid;
use sqlx::PgPool;
use std::cmp::{max, min};
use std::str::FromStr;
use chrono::Local;
use strum::IntoEnumIterator;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, ReplyParameters};

// Generates the inline keyboard for applications with pagination
fn get_applications_keyboard(
    prefix: &String,
    applications: &Vec<Apply>,
    start: usize,
    show: usize
) -> Result<InlineKeyboardMarkup, ()> {
    let total = applications.len();
    let slice_end = min(start + show, total);
    let shown_entries = match applications.get(start..slice_end) {
        Some(entries) => entries,
        None => {
            log::error!("Cannot get applications slice");
            return Err(());
        }
    };

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries
        .iter()
        .map(|entry| {
            vec![InlineKeyboardButton::callback(
                entry.ops_name.clone(),
                format!("{}{}", prefix, entry.id)
            )]
        })
        .collect();

    // Add "PREV", "NEXT", and "CANCEL" buttons
    let mut pagination = Vec::new();
    if start > 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", "PREV"));
    }
    if slice_end < total {
        pagination.push(InlineKeyboardButton::callback("NEXT", "NEXT"));
    }
    pagination.push(InlineKeyboardButton::callback("CANCEL", "CANCEL"));

    entries.push(pagination);

    Ok(InlineKeyboardMarkup::new(entries))
}

// Generates the message text for applications with pagination
fn get_applications_text(
    start: usize,
    slice_end: usize,
    total: usize
) -> String {
    format!("Showing applications {} to {} of {}\nUpdated: {}", start + 1, slice_end, total, Local::now().format("%d%m %H%M.%S").to_string())
}

// Displays applications with pagination using message editing
async fn display_applications(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    applications: &Vec<Apply>,
    prefix: &String,
    start: usize,
    show: usize,
    msg_id: Option<MessageId>, // Optionally provide MessageId to edit
) -> Result<Option<MessageId>, ()> {
    let total = applications.len();
    let slice_end = min(start + show, total);

    // Generate the inline keyboard
    let markup = match get_applications_keyboard(prefix, applications, start, show) {
        Ok(kb) => kb,
        Err(_) => {
            send_msg(
                bot.send_message(chat_id, "Error encountered while generating keyboard."),
                username,
            ).await;
            return Err(());
        }
    };

    // Generate the message text
    let message_text = get_applications_text(start, slice_end, total);

    // Send or edit the message
    Ok(send_or_edit_msg(&bot, chat_id, username, msg_id, message_text, Some(markup), None).await)
}

// Handles re-showing options during pagination
async fn handle_re_show_options(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    applications: Vec<Apply>,
    prefix: String,
    start: usize,
    show: usize,
    msg_id: MessageId, // Existing MessageId to edit
) -> HandlerResult {
    match display_applications(
        bot,
        dialogue.chat_id(),
        username,
        &applications,
        &prefix,
        start,
        show,
        Some(msg_id),
    ).await {
        Ok(msg_id) => {
            match msg_id {
                None => dialogue.update(State::ErrorState).await?,
                Some(new_msg_id) => {
                    log::debug!("Updated ApplyView with MsgId: {:?}, Start: {}", msg_id, start);
                    dialogue.update(State::ApplyView { msg_id: new_msg_id, applications, prefix, start }).await?
                }
            }
        }
        Err(_) => dialogue.update(State::ErrorState).await?,
    };
    Ok(())
}

fn get_application_edit_text(application: &Apply, admin: bool) -> String {
    format!("Details of application:\nNAME: *{}*\nOPS NAME: `{}`\nROLE: `{}`\nTYPE: `{}`\nIS ADMIN: *{}*\nSUBMITTED: _{}_\nUSERNAME: {}\n\nUpdated: _{}_\nDo you wish to edit any entries?",
            application.name,
            application.ops_name,
            application.role_type.as_ref(),
            application.usr_type.as_ref(),
            if admin == true { "YES" } else { "NO" },
            utils::escape_special_characters(&application.created.with_timezone(&Local).format("%b-%d-%Y %H:%M:%S").to_string()),
            format!("[{}](tg://user?id={})", utils::escape_special_characters(&application.chat_username), application.tele_id as u64),
            utils::escape_special_characters(&Local::now().format("%d%m %H%M.%S").to_string())
    )
}

async fn display_application_edit_prompt(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    application: &Apply,
    admin: bool,
    edit_id: Option<MessageId>
) -> Option<MessageId> {
    let mut options: Vec<Vec<InlineKeyboardButton>> = ["NAME", "OPS NAME", "ROLE", "TYPE", "ADMIN"]
        .into_iter()
        .map(|option| vec![InlineKeyboardButton::callback(option, option)])
        .collect();
    let confirm = ["REJECT", "DONE", "CANCEL"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    options.push(confirm);
    
    // Send or edit message
    send_or_edit_msg(bot, chat_id, username, edit_id, get_application_edit_text(&application, admin), Some(InlineKeyboardMarkup::new(options)), Some(ParseMode::MarkdownV2)).await
}

async fn display_edit_role_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    let roles = RoleType::iter()
        .map(|role| InlineKeyboardButton::callback(role.as_ref(), role.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Select role:")
            .reply_markup(InlineKeyboardMarkup::new([roles])),
        username
    ).await
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

pub(super) async fn approve(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "approve", "Command", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Retrieve all the pending applications
    match controllers::apply::get_all_apply_requests(&pool)
        .await {
        Ok(applications) => {
            if applications.is_empty() {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "No pending applications")
                        .reply_parameters(ReplyParameters::new(msg.id)),
                    &user.username
                ).await;
                dialogue.update(State::Start).await?;
                return Ok(());
            }

            send_msg(
                bot.send_message(
                    dialogue.chat_id(),
                    format!("Pending applications: {}", applications.len())
                )
                    .reply_parameters(ReplyParameters::new(msg.id)),
                &user.username
            ).await;

            // Generate random prefix to make the IDs only applicable to this dialogue instance
            let prefix: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(5)
                .map(char::from)
                .collect();

            match display_applications(&bot, dialogue.chat_id(), &user.username, &applications, &prefix, 0, 8, None)
                .await {
                Ok(msg_id) => {
                    match msg_id {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => {
                            log::debug!("Transitioning to ApplyView with MsgId: {:?}, Applications: {:?}, Prefix: {:?}, Start: {:?}", msg_id, applications, prefix, 0);
                            dialogue.update(State::ApplyView { msg_id: new_msg_id, applications, prefix, start: 0 }).await?
                        }
                    }
                },
                Err(_) => dialogue.update(State::ErrorState).await?
            };
        },
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }

    Ok(())
}

pub(super) async fn apply_view(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, applications, prefix, start): (MessageId, Vec<Apply>, String, usize),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_view", "Callback", q,
        "Applications" => applications,
        "Prefix" => prefix,
        "Start" => start
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
            handle_re_show_options(&bot, &dialogue, &q.from.username, applications, prefix, start, 8, msg_id).await?;
        }
        Some(option) => {
            if option == "PREV" {
                handle_re_show_options(&bot, &dialogue, &q.from.username, applications, prefix, max(0, start as i64 - 8) as usize, 8, msg_id).await?;
            } else if option == "NEXT" {
                let entries_len = applications.len();
                handle_re_show_options(&bot, &dialogue, &q.from.username, applications, prefix, if start+8 < entries_len { start+8 } else { start }, 8, msg_id).await?;
            } else if option == "CANCEL" {
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                    &q.from.username,
                ).await;
                dialogue.update(State::Start).await?
            } else {
                match option.strip_prefix(&prefix) {
                    None => handle_re_show_options(&bot, &dialogue, &q.from.username, applications, prefix, start, 8, msg_id).await?,
                    Some(id) => {
                        match Uuid::try_parse(&id) {
                            Ok(parsed_id) => {
                                match controllers::apply::get_apply_by_uuid(&pool, parsed_id).await {
                                    Ok(application) => {
                                        match display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, false, Some(msg_id)).await {
                                            None => {}
                                            Some(msg_id) => {
                                                log::debug!("Transitioning to ApplyEditPrompt with Application: {:?}, Admin: {:?}", application, false );
                                                dialogue.update(State::ApplyEditPrompt { msg_id, application, admin: false }).await?;
                                            }
                                        };
                                    }
                                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                                }
                            }
                            Err(_) => handle_re_show_options(&bot, &dialogue, &q.from.username, applications, prefix, start, 8, msg_id).await?,
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn apply_edit_prompt(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, application, admin): (MessageId, Apply, bool),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_prompt", "Callback", q,
        "MessageId" => msg_id,
        "Application" => application,
        "Admin" => admin
    );

    // Acknowledge the callback to remove the loading state
    if let Err(e) = bot.answer_callback_query(q.id).await {
        log::error!("Failed to answer callback query: {}", e);
    }

    match q.data {
        None => {
            // Invalid option selected
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
        Some(option) => match option.as_str() {
            "DONE" => {
                // Remove the application
                match controllers::apply::remove_apply_by_uuid(&pool, application.id).await {
                    Ok(_) => {
                        // Add the user to the database
                        match controllers::user::add_user(
                            &pool,
                            application.tele_id as u64,
                            application.name,
                            application.ops_name,
                            application.role_type,
                            application.usr_type,
                            admin,
                        )
                            .await
                        {
                            Ok(user) => {
                                // Emit system notification to indicate who has approved the user
                                
                                // Fetch the user's chat info dynamically using getChat
                                let user_chat = bot.get_chat(ChatId(user.tele_id)).await;
                                let has_username = match user_chat {
                                    Ok(chat) => {
                                        if let Some(username) = chat.username() {
                                            // If username exists, mention with username and link
                                            format!("\nUSERNAME: @{}", utils::escape_special_characters(&username))
                                        } else { "".into() }
                                    },
                                    Err(_) => { "" .into() }
                                };
                                notifier::emit::system_notifications(
                                    &bot,
                                    format!(
                                        "{} has approved the application:\nOPS NAME: `{}`\nNAME: *{}*{}",
                                        utils::username_link_tag(&q.from),
                                        user.ops_name,
                                        format!("[{}](tg://user?id={})", user.name, user.tele_id as u64),
                                        has_username
                                    ).as_str(),
                                    &pool,
                                    q.from.id.0 as i64
                                ).await;
                                
                                // If the user is an admin, configure default notification settings
                                if admin {
                                    // Set default notification settings
                                    match controllers::notifications::update_notification_settings(
                                        &pool,
                                        user.tele_id as i64, // Assuming chat_id == tele_id
                                        Some(true),  // notif_system
                                        Some(true),  // notif_register
                                        None,        // notif_availability
                                        Some(true),  // notif_plan
                                        Some(true),  // notif_conflict
                                    ).await {
                                        Ok(_) => {
                                            log::info!("Default notification settings configured for admin user {}", &user.name);
                                        }
                                        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                                    }

                                    // Inform the admin about notification settings
                                    send_msg(
                                        bot.send_message(ChatId(user.tele_id),
                                            "Registered successfully as admin with default notification settings enabled. You can configure your notifications using /notify.", ),
                                        &q.from.username,
                                    ).await;
                                } else {
                                    // Inform regular users
                                    send_msg(
                                        bot.send_message(ChatId(user.tele_id),
                                            "Registered successfully. Use /help to see available actions.", ),
                                        &q.from.username,
                                    ).await;
                                }
                                
                                let message_text = format!(
                                    "Approved application:\nNAME: *{}*\nOPS NAME: `{}`\nROLE: `{}`\nTYPE: `{}`\nIS ADMIN: *{}*\nADDED: _{}_\nUSERNAME: {}", utils::escape_special_characters(&user.name),
                                    utils::escape_special_characters(&user.ops_name),
                                    user.role_type.as_ref(),
                                    user.usr_type.as_ref(),
                                    if admin == true { "YES" } else { "NO" },
                                    utils::escape_special_characters(&user.created.with_timezone(&Local).format("%b-%d-%Y %H:%M:%S").to_string()),
                                    format!("[{}](tg://user?id={})", utils::escape_special_characters(&application.chat_username), application.tele_id as u64)
                                );
                                // Send or edit message
                                send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), message_text, None, Some(ParseMode::MarkdownV2)).await;
                                
                                dialogue.update(State::Start).await?;
                            }
                            Err(e) => {
                                log::error!("Error adding user: {}", e);
                                handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Error removing application by UUID {}: {}", application.id, e);
                        handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                    }
                }
            }
            "CANCEL" => {
                // Operation cancelled
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                    &q.from.username,
                )
                    .await;
                dialogue.update(State::Start).await?
            }
            "REJECT" => {
                // Operation cancelled
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                match controllers::apply::remove_apply_by_uuid(&pool, application.id).await {
                    Ok(success) => {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), if success { "Application rejected." } else { "Error occurred" }),
                            &q.from.username,
                        ).await;
                        dialogue.update(State::Start).await?
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                }
            }
            "NAME" => {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                // Edit name
                match display_edit_name(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(change_msg_id) => dialogue.update(State::ApplyEditName { msg_id, change_msg_id, application, admin }).await?
                }
            }
            "OPS NAME" => {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                // Edit OPS name
                match display_edit_ops_name(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(change_msg_id) => dialogue.update(State::ApplyEditOpsName { msg_id, change_msg_id, application, admin }).await?
                }
            }
            "ROLE" => {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                // Edit role
                match display_edit_role_types(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(change_msg_id) => dialogue.update(State::ApplyEditRole { msg_id, change_msg_id, application, admin }).await?
                }
            }
            "TYPE" => {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                // Edit user type
                match display_edit_user_types(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(change_msg_id) => dialogue.update(State::ApplyEditType { msg_id, change_msg_id, application, admin }).await?
                }
            }
            "ADMIN" => {
                log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                // Edit admin status
                match display_edit_admin(&bot, dialogue.chat_id(), &q.from.username).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(change_msg_id) => dialogue.update(State::ApplyEditAdmin { msg_id, change_msg_id, application, admin }).await?
                }
            }
            _ => {
                // Handle any other invalid options
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Invalid option."),
                    &q.from.username,
                ).await;
            }
        },
    }

    Ok(())
}

pub(super) async fn apply_edit_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (msg_id, change_msg_id, mut application, admin): (MessageId, MessageId, Apply, bool)
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_name", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Application" => application,
        "Admin" => admin
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
                    application.name = name.clone();
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Name updated to: {}", name)),
                        &user.username,
                    ).await;
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    match display_application_edit_prompt(&bot, dialogue.chat_id(), &user.username, &application, admin, Some(msg_id)).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => dialogue.update(State::ApplyEditPrompt { msg_id: new_msg_id, application, admin }).await?
                    }
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
                bot.send_message(dialogue.chat_id(), "Please, enter a full name, or type /cancel to abort."),
                &user.username,
            ).await;
            display_edit_name(&bot, dialogue.chat_id(), &user.username).await;
        }
    }

    Ok(())
}

pub(super) async fn apply_edit_ops_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (msg_id, change_msg_id, mut application, admin): (MessageId, MessageId, Apply, bool),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_ops_name", "Message", msg,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Application" => application,
        "Admin" => admin
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
                    application.ops_name = ops_name.clone();
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    match display_application_edit_prompt(&bot, dialogue.chat_id(), &user.username, &application, admin, Some(msg_id)).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => dialogue.update(State::ApplyEditPrompt { msg_id: new_msg_id, application, admin }).await?
                    }
                }
                Err(_) => {
                    // Let the user retry, or will auto transition to error state if database error occured
                    return Ok(());
                }
            }
        }
        None => {
            // If no text is found in the message, prompt the user to send their OPS name
            send_msg(
                bot.send_message(
                    dialogue.chat_id(),
                    "Please, enter a OPS NAME, or type /cancel to abort.",
                ),
                &user.username,
            ).await;
            display_edit_ops_name(&bot, dialogue.chat_id(), &user.username).await;
        }
    }

    Ok(())
}

pub(super) async fn apply_edit_role(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, mut application, admin): (MessageId, MessageId, Apply, bool),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_role", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Application" => application,
        "Admin" => admin
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
        Some(roletype) => {
            log::debug!("Received input: {:?}", &roletype);
            match RoleType::from_str(&roletype) {
                Ok(role_type_enum) => {
                    log::debug!("Selected role type: {:?}", role_type_enum);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), format!("Selected role type: {}", role_type_enum.as_ref())),
                        &q.from.username,
                    ).await;

                    application.role_type = role_type_enum;
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    match display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, admin, Some(msg_id)).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => dialogue.update(State::ApplyEditPrompt { msg_id: new_msg_id, application, admin }).await?
                    }
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

pub(super) async fn apply_edit_type(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, mut application, admin): (MessageId, MessageId, Apply, bool),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_type", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Application" => application,
        "Admin" => admin
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

                    application.usr_type = user_type_enum;
                    log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                    match display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, admin, Some(msg_id)).await {
                        None => dialogue.update(State::ErrorState).await?,
                        Some(new_msg_id) => dialogue.update(State::ApplyEditPrompt { msg_id: new_msg_id, application, admin }).await?
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

pub(super) async fn apply_edit_admin(
    bot: Bot,
    dialogue: MyDialogue,
    (msg_id, change_msg_id, application, admin): (MessageId, MessageId, Apply, bool),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_admin", "Callback", q,
        "MessageId" => msg_id,
        "Change MessageId" => change_msg_id,
        "Application" => application,
        "Admin" => admin
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
                log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                match display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, true, Some(msg_id)).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(new_msg_id) => dialogue.update(State::ApplyEditPrompt { msg_id: new_msg_id, application, admin: true }).await?
                };
            } else if make_admin_input == "NO" {
                log_try_delete_msg(&bot, dialogue.chat_id(), change_msg_id).await;
                match display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, false, Some(msg_id)).await {
                    None => dialogue.update(State::ErrorState).await?,
                    Some(new_msg_id) => dialogue.update(State::ApplyEditPrompt { msg_id: new_msg_id, application, admin: false }).await?
                };
            } else {
                log::error!("Invalid set admin input received: {}", make_admin_input);
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                    &q.from.username,
                ).await;
            }
        }
    }

    Ok(())
}