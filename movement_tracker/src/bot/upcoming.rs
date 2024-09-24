use sqlx::PgPool;

use teloxide::prelude::*;

use crate::bot::{handle_error, send_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::{controllers, log_endpoint_hit, utils};
use crate::types::UsrType;

pub(super) async fn upcoming(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "upcoming", "Command", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    // Get the availability for user in the database
    match controllers::scheduling::get_planned_availability_details_by_tele_id(&pool, user.id.0).await{
        Ok(availability_list) => {
            // Format the availability list into a readable text message
            let message_text = if availability_list.is_empty() {
                "You have no upcoming planned availabilities\\.".to_string()
            } else {
                // Initialize the message with a header
                let mut message = format!("*Upcoming Planned Availabilities for {}:*\n\n",
                                          utils::escape_special_characters(&user.username.as_deref().unwrap_or("You"))
                );

                // Iterate over each availability entry and append formatted details
                for availability in availability_list {
                    // Format the date (e.g., "Sep 20, 2024")
                    let date_str = availability.avail.format("%b %d\\, %Y").to_string();

                    // ICT Type (e.g., "Type A")
                    let ict_type_str = availability.ict_type.as_ref();

                    // Remarks, truncated to 20 characters for brevity
                    let remarks_str = if let Some(remarks) = &availability.remarks {
                        if remarks.chars().count() > 20 {
                            format!("{}...", &utils::escape_special_characters(remarks.chars().take(20).collect::<String>().as_str()))
                        } else {
                            utils::escape_special_characters(remarks)
                        }
                    } else {
                        "None".to_string()
                    };

                    // SAF100 Status
                    let saf100_str = match availability.usr_type {
                        UsrType::NS => {
                            if availability.saf100 {
                                "*SAF100 Issued*"
                            } else if availability.planned {
                                "*SAF100 Pending*"
                            } else {
                                ""
                            }
                        }
                        _ => "",
                    };

                    // Compile the entry into a formatted string
                    message.push_str(&format!(
                        "\\- *Date*: {}\n  *ICT Type*: {}\n  *Remarks*: {}\n  {}\n\n",
                        date_str,
                        ict_type_str,
                        remarks_str,
                        saf100_str
                    ));
                }

                message
            };

            // Send the formatted message to the user with MarkdownV2 parsing
            send_msg(
                bot.send_message(dialogue.chat_id(), message_text)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2),
                &user.username
            ).await;
        },
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }

    Ok(())
}