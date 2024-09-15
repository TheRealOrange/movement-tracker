use sqlx::PgPool;
use teloxide::{Bot, dptree};
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::dispatching::Dispatcher;
use teloxide::payloads::{SendMessage, SendMessageSetters};
use teloxide::prelude::{ChatId, Dialogue, Requester};
use teloxide::requests::JsonRequest;
use teloxide::types::MessageId;
use state::State;
use crate::bot::state::schema;

pub(self) mod commands;
pub(self) mod user;
pub(self) mod state;
pub(self) mod register;


pub(self) type MyDialogue = Dialogue<State, InMemStorage<State>>;
pub(self) type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

pub(crate) async fn init_bot(pool: PgPool) {
    let bot = Bot::from_env();

    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![
            InMemStorage::<State>::new(), // Provide the InMemStorage in Arc
            pool                                      // Provide the PgPool as well
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
