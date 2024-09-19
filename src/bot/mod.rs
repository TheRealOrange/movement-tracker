use crate::bot::state::schema;
use sqlx::PgPool;
use state::State;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::dispatching::Dispatcher;
use teloxide::payloads::SendMessage;
use teloxide::prelude::{ChatId, Dialogue, Requester};
use teloxide::requests::JsonRequest;
use teloxide::{dptree, Bot};

pub(self) mod commands;
pub(self) mod user;
pub(self) mod state;
pub(self) mod register;
pub(self) mod apply;
pub(self) mod availability;
pub(self) mod forecast;
pub(self) mod notify;
pub(self) mod plan;

pub(self) type MyDialogue = Dialogue<State, InMemStorage<State>>;
pub(self) type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

pub(crate) async fn init_bot(pool: PgPool) {
    let bot = Bot::from_env();

    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(), // Provide the InMemStorage in Arc
            pool                          // Provide the PgPool as well
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

pub(self) async fn send_msg(msg: JsonRequest<SendMessage>, username: &Option<String>) {
    match msg
        .await {
        Ok(_) => {}
        Err(e) => {
            log::error!(
                "Error replying to msg from user: {}, error: {}",
                username.as_deref().unwrap_or("none"),
                e
            );
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

