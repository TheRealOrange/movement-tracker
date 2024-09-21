use std::sync::Arc;
use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::PgPool;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::prelude::*;
use teloxide::types::{ChatKind, InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, User};
use crate::bot::{handle_error, log_try_remove_markup, send_msg, HandlerResult, MyDialogue};
use crate::{controllers, log_endpoint_hit, utils};
use crate::bot::state::State;
use crate::types::NotificationSettings;

fn format_notification_settings(settings: &NotificationSettings) -> String {
    format!(
        "\\- System Notifications: {}\n\\- Register Notifications: {}\n\\- Availability Notifications: {}\n\\- Plan Notifications: {}\n\\- Conflict Notifications: {}",
        if settings.notif_system { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_register { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_availability { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_plan { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_conflict { "🟢 *ON*" } else { "🔴 *OFF*" },
    )
}

fn create_inline_keyboard(settings: &NotificationSettings, prefix: &String) -> InlineKeyboardMarkup {
    let mut buttons: Vec<Vec<InlineKeyboardButton>> = 
        [
            ("SYSTEM", settings.notif_system),
            ("REGISTER", settings.notif_register),
            ("AVAILABILITY", settings.notif_availability),
            ("PLAN", settings.notif_plan),
            ("CONFLICT", settings.notif_conflict)
        ].into_iter()
        .map(|(field, status)| vec![
            InlineKeyboardButton::callback(
                format!(
                    "{}: {}", field,
                    if !status { "ENABLE" } else { "DISABLE" }
                ),
                prefix.clone()+field,
            ),
        ])
        .collect();
    buttons.push(["CANCEL", "CONFIRM"].into_iter().map(|text| InlineKeyboardButton::callback(text, prefix.clone()+text)).collect());
    buttons.push(vec![InlineKeyboardButton::callback("DISABLE ALL", prefix.clone()+"DISABLE ALL")]);

    InlineKeyboardMarkup::new(buttons)
}

async fn display_inchat_config_notification(bot: &Bot, chat_id: ChatId, user: &User) {
    send_msg(
        bot.send_message(chat_id, format!(
            "{} is configuring notification settings for this chat",
            utils::username_link_tag(&user)
        )).parse_mode(ParseMode::MarkdownV2),
        &user.username,
    ).await;
}

async fn display_dm_config_notification(bot: &Bot, chat_id: ChatId, username: &Option<String>, notification_settings: &NotificationSettings, prefix: &String) -> Option<MessageId> {
    let message_text = format!(
        "Configure the notification settings for the chat:\n{}\n\n*Use the buttons below to toggle these settings\\.*",
        format_notification_settings(&notification_settings)
    );

    let keyboard = create_inline_keyboard(&notification_settings, prefix);

    send_msg(
        bot.send_message(chat_id, message_text.as_str())
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .reply_markup(keyboard),
        username,
    ).await
}

async fn update_dm_config_notification(bot: &Bot, chat_id: ChatId, message_id: &MessageId, username: &Option<String>, notification_settings: &NotificationSettings, prefix: &String) {
    let message_text = format!(
        "Configure the notification settings for the chat:\n{}",
        format_notification_settings(&notification_settings)
    );

    let keyboard = create_inline_keyboard(&notification_settings, prefix);

    // Edit both text and reply markup in one call
    match bot.edit_message_text(chat_id, *message_id, message_text.as_str())
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .reply_markup(keyboard)
        .await {
        Ok(_) => {}
        Err(e) => {
            log::error!(
                "Error editing msg in response to user: {}, error: {}",
                username.as_deref().unwrap_or("none"),
                e
            );
        }
    };
}

pub(super) async fn notify(
    bot: Bot,
    dialogue: MyDialogue,
    storage: Arc<InMemStorage<State>>,
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "notify", "Command", msg);

    // Extract user information
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        send_msg(
            bot.send_message(msg.chat.id, "Unable to identify you. Please try again."),
            &None,
        ).await;
        return Ok(());
    };

    // Announce in the chat if the chat is not the user DM
    if let ChatKind::Private(_) = msg.chat.kind {
        // The chat is private (a DM).
    } else {
        // The chat is not a DM (could be a group, supergroup, or channel).
        display_inchat_config_notification(&bot, dialogue.chat_id(), user).await;
    }

    // Fetch existing notification settings for the chat
    let settings = match controllers::notifications::get_notification_settings(&pool, msg.chat.id.0).await {
        Ok(Some(settings)) => settings,
        Ok(None) => {
            // If no settings exist, create default settings
            controllers::notifications::update_notification_settings(
                &pool,
                msg.chat.id.0,
                Some(false),
                Some(false),
                Some(false),
                Some(false),
                Some(false),
            ).await?
        },
        Err(e) => {
            log::error!("Error fetching notification settings: {}", e);
            send_msg(
                bot.send_message(msg.chat.id, "Failed to retrieve notification settings."),
                &user.username,
            ).await;
            return Ok(());
        }
    };

    // Generate random prefix to make the IDs only applicable to this dialogue instance
    let prefix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect();

    // Send DM to the user with current settings
    let msg_id = match display_dm_config_notification(&bot, ChatId(user.id.0 as i64), &user.username, &settings, &prefix).await {
        Some(msg_id) => msg_id,
        None => return Ok(())
    };
    // change DM state
    MyDialogue::new(storage, ChatId(user.id.0 as i64)).update(State::NotifySettings { notification_settings: settings, chat_id: dialogue.chat_id(), prefix, msg_id }).await?;

    Ok(())
}

pub(super) async fn notify_settings(
    bot: Bot,
    dialogue: MyDialogue,
    (mut notification_settings, chat_id, prefix, msg_id): (NotificationSettings, ChatId, String, MessageId),
    q: CallbackQuery,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(
        dialogue.chat_id(), "notify_settings", "Callback", q,
        "NotificationSettings" => notification_settings,
        "ChatId" => chat_id,
        "Prefix" => prefix
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
            match option.strip_prefix(&prefix) {
                None => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Invalid option."),
                        &q.from.username,
                    ).await;
                }
                Some(option) => {
                    if option == "CONFIRM" {
                        // commit to database
                        match controllers::notifications::update_notification_settings(
                            &pool, chat_id.0, 
                            Some(notification_settings.notif_system),
                            Some(notification_settings.notif_register),
                            Some(notification_settings.notif_availability),
                            Some(notification_settings.notif_plan),
                            Some(notification_settings.notif_conflict)
                        ).await {
                            Ok(settings) => {
                                match bot.edit_message_text(dialogue.chat_id(), msg_id, format!(
                                    "Updated notification settings for chat:\n{}",
                                    format_notification_settings(&settings)
                                )).parse_mode(teloxide::types::ParseMode::MarkdownV2).await {
                                    Ok(_) => {}
                                    Err(_) => {
                                        log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                                        send_msg(
                                            bot.send_message(dialogue.chat_id(), format!(
                                                "Updated notification settings for chat:\n{}",
                                                format_notification_settings(&settings)
                                            )).parse_mode(teloxide::types::ParseMode::MarkdownV2),
                                            &q.from.username,
                                        ).await;
                                    }
                                };
                                
                                if dialogue.chat_id() != chat_id {
                                    send_msg(
                                        bot.send_message(chat_id, format!(
                                            "{} has updated notification settings for chat:\n{}",
                                            utils::username_link_tag(&q.from),
                                            format_notification_settings(&settings)
                                        )).parse_mode(teloxide::types::ParseMode::MarkdownV2),
                                        &q.from.username,
                                    ).await;
                                }
                                dialogue.update(State::Start).await?;
                                return Ok(());
                            }
                            Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await,
                        }
                    } else if option == "CANCEL" {
                        log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "Operation cancelled."),
                            &q.from.username,
                        ).await;

                        if dialogue.chat_id() != chat_id {
                            send_msg(
                                bot.send_message(chat_id, format!(
                                    "{} has aborted updating notifications",
                                    utils::username_link_tag(&q.from)  // Use first name and user ID if no username
                                )).parse_mode(teloxide::types::ParseMode::MarkdownV2),
                                &q.from.username,
                            ).await;
                        }
                        dialogue.update(State::Start).await?;
                        return Ok(());
                    } else if option == "DISABLE ALL" {
                        match controllers::notifications::soft_delete_notification_settings(&pool, chat_id.0).await {
                            Ok(_) => {
                                send_msg(
                                    bot.send_message(dialogue.chat_id(), "Notifications disabled"),
                                    &q.from.username,
                                ).await;

                                if dialogue.chat_id() != chat_id {
                                    send_msg(
                                        bot.send_message(chat_id, format!(
                                            "{} has disabled notifications",
                                            utils::username_link_tag(&q.from)
                                        )).parse_mode(teloxide::types::ParseMode::MarkdownV2),
                                        &q.from.username,
                                    ).await;
                                }
                                dialogue.update(State::Start).await?;
                                return Ok(());
                            }
                            Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
                        }
                    } else if option == "SYSTEM" {
                        notification_settings.notif_system = !notification_settings.notif_system;
                    } else if option == "REGISTER" {
                        notification_settings.notif_register = !notification_settings.notif_register;
                    } else if option == "AVAILABILITY" {
                        notification_settings.notif_availability = !notification_settings.notif_availability;
                    } else if option == "PLAN" {
                        notification_settings.notif_plan = !notification_settings.notif_plan;
                    } else if option == "CONFLICT" {
                        notification_settings.notif_conflict = !notification_settings.notif_conflict;
                    } else {
                        send_msg(
                            bot.send_message(dialogue.chat_id(), "Invalid option."),
                            &q.from.username,
                        ).await;
                        return Ok(());
                    }

                    update_dm_config_notification(&bot, dialogue.chat_id(), &msg_id, &q.from.username, &notification_settings, &prefix).await;
                    dialogue.update(State::NotifySettings { notification_settings, chat_id, prefix, msg_id }).await?;
                }
            }
        }
    }

    Ok(())
}