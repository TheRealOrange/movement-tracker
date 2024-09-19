use sqlx::PgPool;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ReplyParameters};
use crate::bot::{send_msg, HandlerResult, MyDialogue};
use crate::{controllers, log_endpoint_hit, utils};
use crate::bot::state::State;
use crate::types::NotificationSettings;

fn format_notification_settings(settings: &NotificationSettings) -> String {
    format!(
        "- System Notifications: {}\n- Register Notifications: {}\n- Availability Notifications: {}\n- Plan Notifications: {}\n- Conflict Notifications: {}\n\n\
        *Use the buttons below to toggle these settings.*",
        if settings.notif_system { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_register { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_availability { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_plan { "🟢 *ON*" } else { "🔴 *OFF*" },
        if settings.notif_conflict { "🟢 *ON*" } else { "🔴 *OFF*" },
    )
}

fn create_inline_keyboard(settings: &NotificationSettings) -> InlineKeyboardMarkup {
    let buttons = vec![
        vec![
            InlineKeyboardButton::callback(
                format!(
                    "SYSTEM: {}",
                    if settings.notif_system { "ENABLE" } else { "DISABLE" }
                ),
                "SYSTEM",
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!(
                    "REGISTER: {}",
                    if settings.notif_register { "ENABLE" } else { "DISABLE" }
                ),
                "REGISTER",
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!(
                    "AVAILABILITY: {}",
                    if settings.notif_availability { "ENABLE" } else { "DISABLE" }
                ),
                "AVAILABILITY",
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!(
                    "PLAN: {}",
                    if settings.notif_plan { "ENABLE" } else { "DISABLE" }
                ),
                "PLAN",
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!(
                    "CONFLICT: {}",
                    if settings.notif_conflict { "ENABLE" } else { "DISABLE" }
                ),
                "CONFLICT",
            ),
        ],
        vec![InlineKeyboardButton::callback("CANCEL", "CANCEL"), InlineKeyboardButton::callback("DONE", "DONE")],
        vec![InlineKeyboardButton::callback("DISABLE ALL", "DISABLE ALL")]
    ];

    InlineKeyboardMarkup::new(buttons)
}

async fn display_inchat_config_notification(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    send_msg(
        bot.send_message(chat_id, format!(
            "{} is configuring notification settings for this chat", 
            username.as_deref().unwrap_or("none")
        )),
        username,
    ).await;
}

async fn display_dm_config_notification(bot: &Bot, chat_id: ChatId, username: &Option<String>, notification_settings: &NotificationSettings) {
    let message_text = format_notification_settings(&notification_settings);

    let keyboard = create_inline_keyboard(&notification_settings);

    send_msg(
        bot.send_message(chat_id, utils::escape_special_characters(message_text.as_str()))
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .reply_markup(keyboard),
        username,
    ).await;
}

pub(super) async fn notify(
    bot: Bot,
    dialogue: MyDialogue,
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

    // Announce in the chat
    display_inchat_config_notification(&bot, dialogue.chat_id(), &user.username).await;

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

    // Send DM to the user with current settings
    display_dm_config_notification(&bot, ChatId(user.id.0 as i64), &user.username, &settings).await;
    dialogue.update(State::NotifySettings { notification_settings: settings, chat_id: dialogue.chat_id() });
    
    Ok(())
}