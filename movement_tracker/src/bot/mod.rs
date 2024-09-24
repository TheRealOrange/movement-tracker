use crate::bot::state::schema;
use sqlx::PgPool;
use state::State;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::dispatching::Dispatcher;
use teloxide::payloads::SendMessage;
use teloxide::prelude::*;
use teloxide::requests::JsonRequest;
use teloxide::{dptree, Bot};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};
use crate::{controllers, utils};

pub(self) mod commands;
pub(self) mod user;
pub(self) mod state;
pub(self) mod register;
pub(self) mod apply;
pub(self) mod availability;
pub(self) mod forecast;
pub(self) mod notify;
pub(self) mod plan;
pub(self) mod upcoming;
mod saf100;

pub(self) type MyDialogue = Dialogue<State, InMemStorage<State>>;
pub(self) type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

pub(crate) async fn init_bot(bot: Bot, pool: PgPool) {
    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(),
            pool
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

pub(self) async fn send_msg(msg: JsonRequest<SendMessage>, username: &Option<String>) -> Option<MessageId> {
    match msg
        .await {
        Ok(msg) => { Some(msg.id) }
        Err(e) => {
            log::error!(
                "Error responding to msg from user: {}, error: {}",
                username.as_deref().unwrap_or("none"),
                e
            );
            None
        }
    }
}

pub(self) async fn send_or_edit_msg(bot: &Bot, chat_id: ChatId, username: &Option<String>, msg_id: Option<MessageId>, message_text: String, markup_input: Option<InlineKeyboardMarkup>, parse_mode_input: Option<ParseMode>) -> Option<MessageId> {
    // Send or edit the message
    match msg_id {
        Some(id) => {
            let empty_keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
            // Edit the existing message
            let mut bot_msg = bot.edit_message_text(chat_id, id, &message_text);
            if let Some(markup) = markup_input.clone() {
                bot_msg = bot_msg.reply_markup(markup);
            } else {
                bot_msg = bot_msg.reply_markup(InlineKeyboardMarkup::new(empty_keyboard));
            }
            if let Some(mode) = parse_mode_input {
                bot_msg = bot_msg.parse_mode(mode);
            }
            
            match bot_msg.await {
                Ok(msg) => Some(msg.id),
                Err(e) => {
                    log::error!("Failed to edit message in chat ({}): {}", chat_id.0, e);
                    log_try_delete_msg(&bot, chat_id, id).await;
                    // Failed to edit message, try to send a new one
                    let mut bot_msg = bot.send_message(chat_id, message_text);
                    if let Some(markup) = markup_input {
                        bot_msg = bot_msg.reply_markup(markup);
                    }
                    if let Some(mode) = parse_mode_input {
                        bot_msg = bot_msg.parse_mode(mode);
                    }
                    send_msg(bot_msg, username).await
                }
            }
        }
        None => {
            // Send a new message
            let mut bot_msg = bot.send_message(chat_id, message_text);
            if let Some(markup) = markup_input {
                bot_msg = bot_msg.reply_markup(markup);
            }
            if let Some(mode) = parse_mode_input {
                bot_msg = bot_msg.parse_mode(mode);
            }
            send_msg(bot_msg, username).await
        }
    }
}

pub(self) async fn handle_error(
    bot: &Bot,
    dialogue: &MyDialogue,
    chat_id: ChatId,
    username: &Option<String>,
) {
    send_msg(
        bot.send_message(chat_id, "Error occurred accessing the database"),
        username,
    )
        .await;
    dialogue.update(State::ErrorState).await.unwrap_or(());
}

pub(self) async fn log_try_delete_msg(bot: &Bot, chat_id: ChatId, msg_id: MessageId) {
    match bot.delete_message(chat_id, msg_id).await {
        Ok(_) => {}
        Err(_) => { log::error!("Failed to delete message ({})", msg_id.0); }
    };
}

pub(self) async fn log_try_remove_markup(bot: &Bot, chat_id: ChatId, msg_id: MessageId) {
    let empty_keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    match bot.edit_message_reply_markup(chat_id, msg_id).reply_markup(InlineKeyboardMarkup::new(empty_keyboard)).await {
        Ok(_) => {}
        Err(_) => { log::error!("Failed to remove message markup ({})", msg_id.0); }
    };
}

pub(self) async fn validate_name(bot: &Bot, dialogue: &MyDialogue, username: &Option<String>, input_name_raw: String, normalize: bool) -> Result<String, ()> {
    let mut cleaned_name = utils::cleanup_name(&input_name_raw);
    // Validate that the name contains only alphabetical characters and spaces
    if !utils::is_valid_name(&cleaned_name) {
        // Invalid input: Notify the user and prompt to re-enter the name
        send_msg(
            bot.send_message(
                dialogue.chat_id(),
                "Invalid name. Please use only letters and spaces. Try again or type /cancel to abort.",
            ),
            username,
        ).await;

        log::debug!(
                    "User {} entered invalid name: {}",
                    username.as_deref().unwrap_or("Unknown"),
                    input_name_raw
                );

        // Remain in the current state to allow the user to re-enter their name
        return Err(());
    }
    
    if normalize {
        // Normalize the name (e.g., capitalize each word)
        cleaned_name = cleaned_name
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
            username,
        ).await;

        // Log the invalid attempt
        log::debug!(
                    "User {} entered name exceeding max length: {}",
                    username.as_deref().unwrap_or("Unknown"),
                    cleaned_name
                );

        return Err(());
    }

    Ok(cleaned_name.to_string())
}

pub(self) async fn validate_ops_name(bot: &Bot, dialogue: &MyDialogue, username: &Option<String>, input_ops_name_raw: String, pool: &PgPool) -> Result<String, ()> {
    let cleaned_ops_name = utils::cleanup_name(&input_ops_name_raw).to_uppercase();

    // Validate that the OPS name contains only allowed characters and is not empty
    if !utils::is_valid_ops_name(&cleaned_ops_name) {
        // Invalid input: Notify the user and prompt to re-enter OPS name
        send_msg(
            bot.send_message(
                dialogue.chat_id(),
                "Invalid OPS NAME. Please use only letters and spaces. Try again or type /cancel to abort.",
            ),
            username,
        ).await;
        // Log the invalid attempt
        log::debug!(
                    "User {} entered invalid OPS name: {}",
                    username.as_deref().unwrap_or("Unknown"),
                    input_ops_name_raw
                );
        // Remain in the current state to allow the user to re-enter OPS name
        return Err(());
    }

    // Enforce a maximum length
    if cleaned_ops_name.len() > utils::MAX_OPS_NAME_LENGTH {
        send_msg(
            bot.send_message(
                dialogue.chat_id(),
                format!(
                    "OPS NAME is too long. Please enter a name with no more than {} characters.",
                    utils::MAX_OPS_NAME_LENGTH
                ),
            ),
            username,
        ).await;

        // Log the invalid attempt
        log::debug!(
                    "User {} entered OPS name exceeding max length: {}",
                    username.as_deref().unwrap_or("Unknown"),
                    cleaned_ops_name
                );

        return Err(());
    }

    // Check for OPS name uniqueness
    match controllers::user::user_exists_ops_name(&pool, &cleaned_ops_name).await {
        Ok(true) => {
            // OPS name already exists: Notify the user and prompt to re-enter
            send_msg(
                bot.send_message(dialogue.chat_id(), "OPS NAME already exists. Please choose a different OPS NAME or type /cancel to abort." ),
                username,
            ).await;
            // Log the duplicate OPS name attempt
            log::debug!(
                        "User {} attempted to use a duplicate OPS name: {}",
                        username.as_deref().unwrap_or("Unknown"),
                        cleaned_ops_name
                    );
            // Remain in the current state to allow the user to re-enter OPS name
            Err(())
        },
        Ok(false) => {
            // OPS name is unique, proceed with registration
            Ok(cleaned_ops_name.to_string())
        },
        Err(_) => {
            handle_error(&bot, &dialogue, dialogue.chat_id(), username).await;
            Err(())
        }
    }
}

async fn retrieve_callback_data(bot: &Bot, chat_id: ChatId, q: &CallbackQuery) -> Result<String, ()> {
    // Extract the callback data
    match q.data.as_ref() {
        Some(d) => Ok(d.clone()),
        None => {
            send_msg(
                bot.send_message(chat_id, "Invalid option."),
                &q.from.username,
            ).await;
            Err(())
        }
    }
}

#[macro_export]
macro_rules! log_endpoint_hit {
    ($chat_id:expr, $fn_name:expr) => {
        log::info!(
            "Chat ID: {} triggered endpoint: {}",
            $chat_id,
            $fn_name
        );
    };
    ($chat_id:expr, $fn_name:expr, $endpoint_type:expr, $data_debug:expr) => {
        log::info!(
            "Chat ID: {} triggered endpoint: {}",
            $chat_id,
            $fn_name
        );
        log::debug!("Endpoint: {}, {}: {:?}", $fn_name, $endpoint_type, $data_debug);
    };
    ($chat_id:expr, $fn_name:expr, $endpoint_type:expr, $data_debug:expr, $( $name:expr => $value:expr ),* ) => {
        log::info!(
            "Chat ID: {} triggered endpoint: {}",
            $chat_id,
            $fn_name
        );
        let extra_info = vec![
            $( format!("{}: {:?}", $name, $value) ),*
        ].join(", ");
        log::debug!(
            "Endpoint: {}, {}, {}: {:?}",
            $fn_name,
            extra_info,
            $endpoint_type,
            $data_debug
        );
    };
}

