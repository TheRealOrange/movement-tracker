use crate::bot::state::State;
use crate::bot::{handle_error, send_msg, HandlerResult, MyDialogue};
use crate::types::{Apply, RoleType, UsrType};
use crate::{controllers, log_endpoint_hit, utils};
use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::types::Uuid;
use sqlx::PgPool;
use std::cmp::{max, min};
use std::str::FromStr;
use chrono::Local;
use strum::IntoEnumIterator;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ReplyParameters};

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
    format!("Showing applications {} to {} of {}", start + 1, slice_end, total)
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
) -> Result<MessageId, ()> {
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
    match msg_id {
        Some(id) => {
            // Edit the existing message
            match bot.edit_message_text(chat_id, id, message_text)
                .reply_markup(markup)
                .await
            {
                Ok(_) => Ok(id),
                Err(e) => {
                    log::error!("Failed to edit message: {}", e);
                    Err(())
                }
            }
        }
        None => {
            // Send a new message
            match send_msg(
                bot.send_message(chat_id, message_text)
                    .reply_markup(markup),
                username
            ).await {
                Some(sent_msg) => Ok(sent_msg),
                None => Err(())
            }
        }
    }
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
            log::debug!("Updated ApplyView with MsgId: {:?}, Start: {}", msg_id, start);
            dialogue.update(State::ApplyView { msg_id, applications, prefix, start }).await?;
        }
        Err(_) => dialogue.update(State::ErrorState).await?,
    };
    Ok(())
}

async fn display_application_edit_prompt(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    application: &Apply,
    admin: bool,
) {
    let mut options: Vec<Vec<InlineKeyboardButton>> = ["NAME", "OPS NAME", "ROLE", "TYPE", "ADMIN"]
        .into_iter()
        .map(|option| vec![InlineKeyboardButton::callback(option, option)])
        .collect();
    let confirm = ["DONE", "CANCEL"]
        .into_iter()
        .map(|option| InlineKeyboardButton::callback(option, option))
        .collect();
    options.push(confirm);

    send_msg(
        bot.send_message(
            chat_id,
            format!("Details of application:\nNAME: {}\nOPS NAME: {}\nROLE: {}\nTYPE: {}\nSUBMITTED: {}\nUSERNAME: {}\nIS ADMIN: {}\n\nDo you wish to edit any entries?",
                &application.name,
                &application.ops_name,
                application.role_type.as_ref(),
                application.usr_type.as_ref(),
                &application.created.with_timezone(&Local).format("%b-%d-%Y %H:%M:%S").to_string(),
                &application.chat_username,
                if admin == true { "YES" } else { "NO" }
            )
        )
            .reply_markup(InlineKeyboardMarkup::new(options)),
        username
    ).await;
}

async fn display_edit_role_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let roles = RoleType::iter()
        .map(|role| InlineKeyboardButton::callback(role.as_ref(), role.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Select role:")
            .reply_markup(InlineKeyboardMarkup::new([roles])),
        username
    ).await;
}

async fn display_edit_user_types(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let usrtypes = UsrType::iter()
        .map(|usrtype| InlineKeyboardButton::callback(usrtype.as_ref(), usrtype.as_ref()));

    send_msg(
        bot.send_message(chat_id, "Select user type:")
            .reply_markup(InlineKeyboardMarkup::new([usrtypes])),
        username
    ).await;
}

async fn display_edit_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    send_msg(
        bot.send_message(chat_id, "Enter full name:"),
        username,
    ).await;
}

async fn display_edit_ops_name(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    send_msg(
        bot.send_message(chat_id, "Enter OPS NAME:"),
        username,
    ).await;
}

async fn display_edit_admin(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let confirm = ["YES", "NO"]
        .map(|product| InlineKeyboardButton::callback(product, product));

    send_msg(
        bot.send_message(chat_id, "Make user admin?")
            .reply_markup(InlineKeyboardMarkup::new([confirm])),
        username
    ).await;
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
                    log::debug!("Transitioning to ApplyView with MsgId: {:?}, Applications: {:?}, Prefix: {:?}, Start: {:?}", msg_id, applications, prefix, 0);
                    dialogue.update(State::ApplyView { msg_id, applications, prefix, start: 0 }).await?
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
                                        display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, false).await;
                                        log::debug!("Transitioning to ApplyEditPrompt with Application: {:?}, Admin: {:?}", application, false );
                                        dialogue.update(State::ApplyEditPrompt { application, admin: false }).await?;
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
    (application, admin): (Apply, bool),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_prompt", "Callback", q,
        "Application" => application,
        "Admin" => admin
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
            display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, admin).await;
        }
        Some(option) => {
            match option.as_str() {
                "DONE" => {
                    match controllers::apply::remove_apply_by_uuid(&pool, application.id)
                        .await {
                        Ok(_) => {
                            match controllers::user::add_user(
                                &pool,
                                application.tele_id as u64,
                                application.name,
                                application.ops_name,
                                application.role_type,
                                application.usr_type,
                                admin
                            ).await {
                                Ok(user) => {
                                    send_msg(
                                        bot.send_message(dialogue.chat_id(), format!(
                                            "Added user\nNAME: {}\nOPS NAME: {}",
                                            user.name, user.ops_name
                                        )),
                                        &q.from.username,
                                    ).await;

                                    dialogue.update(State::Start).await?;
                                }
                                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                            }
                        }
                        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                    }
                }
                "CANCEL" => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                        &q.from.username,
                    ).await;
                    dialogue.update(State::Start).await?
                }
                "NAME" => {
                    display_edit_name(&bot, dialogue.chat_id(), &q.from.username).await;
                    dialogue.update(State::ApplyEditName { application, admin }).await?;
                }
                "OPS NAME" => {
                    display_edit_ops_name(&bot, dialogue.chat_id(), &q.from.username).await;
                    dialogue.update(State::ApplyEditOpsName { application, admin }).await?;
                }
                "ROLE" => {
                    display_edit_role_types(&bot, dialogue.chat_id(), &q.from.username).await;
                    dialogue.update(State::ApplyEditRole { application, admin }).await?;
                }
                "TYPE" => {
                    display_edit_user_types(&bot, dialogue.chat_id(), &q.from.username).await;
                    dialogue.update(State::ApplyEditType { application, admin }).await?;
                }
                "ADMIN" => {
                    display_edit_admin(&bot, dialogue.chat_id(), &q.from.username).await;
                    dialogue.update(State::ApplyEditAdmin { application, admin }).await?;
                }
                _ => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Invalid option."),
                        &q.from.username,
                    ).await;
                    display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, admin).await;
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn apply_edit_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    (mut application, admin): (Apply, bool)
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_name", "Message", msg,
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
            let cleaned_name = utils::cleanup_name(&input_name_raw);

            // Validate that the name contains only alphabetical characters and spaces
            if !utils::is_valid_name(&cleaned_name) {
                // Invalid input: Notify the user and prompt to re-enter the name
                send_msg(
                    bot.send_message(
                        dialogue.chat_id(),
                        "Invalid name. Please use only letters and spaces. Try again or type /cancel to abort.",
                    ),
                    &user.username,
                )
                    .await;
                display_edit_name(&bot, dialogue.chat_id(), &user.username).await;

                log::debug!(
                    "User {} entered invalid name: {}",
                    user.username.as_deref().unwrap_or("Unknown"),
                    input_name_raw
                );

                // Remain in the current state to allow the user to re-enter their name
                return Ok(());
            }

            if cleaned_name.len() > utils::MAX_NAME_LENGTH {
                send_msg(
                    bot.send_message(
                        dialogue.chat_id(),
                        format!(
                            "Name is too long. Please enter a name with no more than {} characters.",
                            utils::MAX_NAME_LENGTH
                        ),
                    ),
                    &user.username,
                ).await;
                display_edit_name(&bot, dialogue.chat_id(), &user.username).await;

                // Log the invalid attempt
                log::debug!(
                    "User {} entered name exceeding max length: {}",
                    user.username.as_deref().unwrap_or("Unknown"),
                    cleaned_name
                );

                return Ok(());
            }

            application.name = cleaned_name.to_string();
            display_application_edit_prompt(&bot, dialogue.chat_id(), &user.username, &application, admin).await;
            dialogue.update(State::ApplyEditPrompt { application, admin }).await?;
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
    (mut application, admin): (Apply, bool),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_ops_name", "Message", msg,
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
            let cleaned_ops_name = utils::cleanup_name(&input_ops_name_raw);

            // Validate that the OPS name contains only allowed characters and is not empty
            if !utils::is_valid_ops_name(&cleaned_ops_name) {
                // Invalid input: Notify the user and prompt to re-enter OPS name
                send_msg(
                    bot.send_message(
                        dialogue.chat_id(),
                        "Invalid OPS NAME. Please use only letters and spaces. Try again or type /cancel to abort.",
                    ),
                    &user.username,
                )
                    .await;
                display_edit_ops_name(&bot, dialogue.chat_id(), &user.username).await;
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
                )
                    .await;
                display_edit_ops_name(&bot, dialogue.chat_id(), &user.username).await;

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
                        bot.send_message(
                            dialogue.chat_id(),
                            "OPS NAME already exists. Please choose a different OPS NAME or type /cancel to abort.",
                        ),
                        &user.username,
                    )
                        .await;
                    display_edit_ops_name(&bot, dialogue.chat_id(), &user.username).await;
                    log::debug!(
                        "User {} attempted to use a duplicate OPS name: {}",
                        user.username.as_deref().unwrap_or("Unknown"),
                        cleaned_ops_name
                    );
                    // Remain in the current state to allow the user to re-enter OPS name
                    return Ok(());
                },
                Ok(false) => {
                    application.ops_name = cleaned_ops_name.clone();
                    display_application_edit_prompt(&bot, dialogue.chat_id(), &user.username, &application, admin).await;
                    dialogue.update(State::ApplyEditPrompt { application, admin }).await?;
                },
                Err(e) => {
                    // Handle unexpected database errors
                    log::error!("Database error during user_exists_ops_name: {}", e);
                    handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await;
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
    (mut application, admin): (Apply, bool),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_role", "Callback", q,
        "Application" => application,
        "Admin" => admin
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;

            display_edit_role_types(&bot, dialogue.chat_id(), &q.from.username).await;
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
                    display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, admin).await;
                    dialogue.update(State::ApplyEditPrompt { application, admin }).await?;
                }
                Err(e) => {
                    log::error!("Invalid role type received: {}", e);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                        &q.from.username,
                    ).await;
                    display_edit_role_types(&bot, dialogue.chat_id(), &q.from.username).await;
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn apply_edit_type(
    bot: Bot,
    dialogue: MyDialogue,
    (mut application, admin): (Apply, bool),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_type", "Callback", q,
        "Application" => application,
        "Admin" => admin
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;

            display_edit_user_types(&bot, dialogue.chat_id(), &q.from.username).await;
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
                    display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, admin).await;
                    dialogue.update(State::ApplyEditPrompt { application, admin }).await?;
                }
                Err(e) => {
                    log::error!("Invalid role type received: {}", e);
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                        &q.from.username,
                    ).await;
                    display_edit_user_types(&bot, dialogue.chat_id(), &q.from.username).await;
                }
            }
        }
    }

    Ok(())
}

pub(super) async fn apply_edit_admin(
    bot: Bot,
    dialogue: MyDialogue,
    (application, admin): (Apply, bool),
    q: CallbackQuery
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "apply_edit_admin", "Callback", q,
        "Application" => application,
        "Admin" => admin
    );

    match q.data {
        None => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;

            display_edit_admin(&bot, dialogue.chat_id(), &q.from.username).await;
        }
        Some(make_admin_input) => {
            log::debug!("Received input: {:?}", &make_admin_input);
            if make_admin_input == "YES" {
                display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, true).await;
                dialogue.update(State::ApplyEditPrompt { application, admin: true }).await?;
            } else if make_admin_input == "NO" {
                display_application_edit_prompt(&bot, dialogue.chat_id(), &q.from.username, &application, false).await;
                dialogue.update(State::ApplyEditPrompt { application, admin: false }).await?;
            } else {
                log::error!("Invalid set admin input received: {}", make_admin_input);
                send_msg(
                    bot.send_message(dialogue.chat_id(), "Please select an option or type /cancel to abort"),
                    &q.from.username,
                ).await;
                display_edit_admin(&bot, dialogue.chat_id(), &q.from.username).await;
            }
        }
    }

    Ok(())
}