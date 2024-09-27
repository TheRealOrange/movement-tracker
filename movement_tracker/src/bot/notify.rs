use std::sync::Arc;

use sqlx::PgPool;

use teloxide::prelude::*;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::types::{ChatKind, InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, User};

use crate::bot::{handle_error, log_try_remove_markup, match_callback_data, retrieve_callback_data, send_msg, send_or_edit_msg, HandlerResult, MyDialogue};
use crate::{controllers, log_endpoint_hit, utils};
use crate::bot::state::State;
use crate::types::NotificationSettings;

use serde::{Serialize, Deserialize};
use strum::EnumProperty;
use callback_data::CallbackData;
use callback_data::CallbackDataHandler;

// Represents callback actions with optional associated data.
#[derive(Debug, Clone, Serialize, Deserialize, EnumProperty, CallbackData)]
enum NotifyCallbackData {
    // Pagination Actions
    SystemNotification { enable: bool },
    RegisterNotification { enable: bool },
    AvailabilityNotification { enable: bool },
    PlanNotification { enable: bool },
    ConflictNotification { enable: bool },

    // Completion Actions
    Cancel,
    Confirm,
    
    // Disable All notifications Action
    DisableAll
}

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
            ("SYSTEM",       settings.notif_system,       NotifyCallbackData::SystemNotification { enable: !settings.notif_system }),
            ("REGISTER",     settings.notif_register,     NotifyCallbackData::RegisterNotification { enable: !settings.notif_register }),
            ("AVAILABILITY", settings.notif_availability, NotifyCallbackData::AvailabilityNotification { enable: !settings.notif_availability }),
            ("PLAN",         settings.notif_plan,         NotifyCallbackData::PlanNotification { enable: !settings.notif_plan }),
            ("CONFLICT",     settings.notif_conflict,     NotifyCallbackData::ConflictNotification { enable: !settings.notif_conflict }),
        ].into_iter()
        .map(|(field, status, data)| vec![
            InlineKeyboardButton::callback(
                format!(
                    "{}: {}", field,
                    if !status { "ENABLE" } else { "DISABLE" }
                ),
                data.to_callback_data(prefix),
            ),
        ])
        .collect();
    buttons.push([("CANCEL", NotifyCallbackData::Cancel), ("CONFIRM", NotifyCallbackData::Confirm)].into_iter()
        .map(|(text, data)| InlineKeyboardButton::callback(text, data.to_callback_data(prefix))).collect());
    buttons.push(vec![InlineKeyboardButton::callback("DISABLE ALL", NotifyCallbackData::DisableAll.to_callback_data(prefix))]);

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

async fn update_dm_config_notification(bot: &Bot, chat_id: ChatId, message_id: &MessageId, username: &Option<String>, notification_settings: &NotificationSettings, prefix: &String) -> Option<MessageId> {
    let message_text = format!(
        "Configure the notification settings for the chat:\n{}",
        format_notification_settings(&notification_settings)
    );

    let keyboard = create_inline_keyboard(&notification_settings, prefix);
    
    // Send or edit message
    send_or_edit_msg(bot, chat_id, username, Some(*message_id), message_text, Some(keyboard), Some(ParseMode::MarkdownV2)).await
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
    let prefix: String = utils::generate_prefix(utils::CALLBACK_PREFIX_LEN);

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
        NotifyCallbackData::Confirm => {
            // commit to database
            return match controllers::notifications::update_notification_settings(
                &pool, chat_id.0,
                Some(notification_settings.notif_system),
                Some(notification_settings.notif_register),
                Some(notification_settings.notif_availability),
                Some(notification_settings.notif_plan),
                Some(notification_settings.notif_conflict)
            ).await {
                Ok(settings) => {
                    let message_text = format!(
                        "Updated notification settings for chat:\n{}",
                        format_notification_settings(&settings)
                    );
                    // Send or edit message
                    send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), message_text, None, Some(ParseMode::MarkdownV2)).await;

                    if dialogue.chat_id() != chat_id {
                        send_msg(
                            bot.send_message(chat_id, format!(
                                "{} has updated notification settings for chat:\n{}",
                                utils::username_link_tag(&q.from),
                                format_notification_settings(&settings)
                            )).parse_mode(ParseMode::MarkdownV2),
                            &q.from.username,
                        ).await;
                    }
                    dialogue.update(State::Start).await?;
                    Ok(())
                }
                Err(_) => {
                    handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await;
                    Ok(())
                },
            }
        }
        NotifyCallbackData::Cancel => {
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
                    )).parse_mode(ParseMode::MarkdownV2),
                    &q.from.username,
                ).await;
            }
            dialogue.update(State::Start).await?;
            return Ok(());
        }
        NotifyCallbackData::DisableAll => {
            return match controllers::notifications::soft_delete_notification_settings(&pool, chat_id.0).await {
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
                    Ok(())
                }
                Err(_) => {
                    handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await;
                    Ok(())
                }
            }
        }
        NotifyCallbackData::SystemNotification { enable } => {
            notification_settings.notif_system = enable;
        }
        NotifyCallbackData::RegisterNotification { enable } => {
            notification_settings.notif_register = enable;
        }
        NotifyCallbackData::AvailabilityNotification { enable } => {
            notification_settings.notif_availability = enable;
        }
        NotifyCallbackData::PlanNotification { enable } => {
            notification_settings.notif_plan = enable;
        }
        NotifyCallbackData::ConflictNotification { enable } => {
            notification_settings.notif_conflict = enable;
        }
    }
    
    // Intentionally continue using the same prefix to handle quick multiple actions
    match update_dm_config_notification(&bot, dialogue.chat_id(), &msg_id, &q.from.username, &notification_settings, &prefix).await {
        None => dialogue.update(State::ErrorState).await?,
        Some(new_msg_id) => dialogue.update(State::NotifySettings { notification_settings, chat_id, prefix, msg_id: new_msg_id }).await?
    }

    Ok(())
}