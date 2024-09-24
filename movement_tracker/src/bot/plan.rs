use std::cmp::{max, min};
use std::collections::HashSet;
use std::str::FromStr;
use chrono::NaiveDate;

use sqlx::PgPool;
use sqlx::types::Uuid;

use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, User};

use super::{handle_error, log_try_remove_markup, match_callback_data, retrieve_callback_data, send_msg, send_or_edit_msg, HandlerResult, MyDialogue};
use crate::bot::state::State;
use crate::types::{AvailabilityDetails, RoleType, Usr, UsrType};
use crate::{controllers, log_endpoint_hit, notifier, utils};
use crate::utils::generate_prefix;

use serde::{Serialize, Deserialize};
use strum::EnumProperty;
use strum::IntoEnumIterator;
use callback_data::CallbackData;
use callback_data::CallbackDataHandler;

// Represents callback actions with optional associated data.
#[derive(Debug, Clone, Serialize, Deserialize, EnumProperty, CallbackData)]
pub enum AvailabilityCallbackData {
    // Pagination Actions
    Prev,
    Next,

    // Completion Actions
    Done,
    Cancel,

    // Role View Change Actions
    ViewRole { role: RoleType },

    // Plan Toggle Actions with associated UUID
    PlanToggle { id: Uuid },

    // Confirmation Actions
    ConfirmYes,
    ConfirmNo,
}

// Generates the inline keyboard for user availability view
fn get_user_availability_keyboard(
    prefix: &String,
    availability_list: &Vec<AvailabilityDetails>,
    changes: &HashSet<Uuid>,
    start: usize,
    show: usize
) -> Result<InlineKeyboardMarkup, ()> {
    let slice_end = min(start + show, availability_list.len());
    let shown_entries = match availability_list.get(start..slice_end) {
        Some(entries) => entries,
        None => {
            log::error!("Cannot get availability entries slice");
            return Err(());
        }
    };

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries
        .iter()
        .map(|entry| {
            // Check if the availability ID is in the changes vector
            let is_in_changes = changes.contains(&entry.id);
            let option_str = if entry.planned ^ is_in_changes {
                if entry.is_valid { "UNPLAN" } else { "UNPLAN (UNAVAIL)" }
            } else {
                "PLAN"
            };
            let truncated_remarks = if let Some(remarks) = &entry.remarks {
                if remarks.chars().count() > 8 {
                    format!(", {}...", remarks.chars().take(8).collect::<String>().as_str())
                } else {
                    format!(", {}", remarks)
                }
            } else {
                "".to_string()
            };
            // Format date as "MMM-DD"
            let formatted = format!(
                "{} {}: {}{}",
                option_str,
                entry.avail.format("%b-%d"),
                entry.ict_type.as_ref(),
                truncated_remarks
            );
            vec![InlineKeyboardButton::callback(
                formatted,
                AvailabilityCallbackData::PlanToggle { id: entry.id }.to_callback_data(prefix),
            )]
        })
        .collect();

    // Add "PREV", "NEXT", and "DONE" buttons
    let mut pagination = Vec::new();
    if start > 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", AvailabilityCallbackData::Prev.to_callback_data(prefix)));
    }
    if slice_end < availability_list.len() {
        pagination.push(InlineKeyboardButton::callback("NEXT", AvailabilityCallbackData::Next.to_callback_data(prefix)));
    }

    entries.push(pagination);
    entries.push(vec![
        InlineKeyboardButton::callback("DONE", AvailabilityCallbackData::Done.to_callback_data(prefix)),
        InlineKeyboardButton::callback("CANCEL", AvailabilityCallbackData::Cancel.to_callback_data(prefix))
    ]);

    Ok(InlineKeyboardMarkup::new(entries))
}

// Generates the inline keyboard for date availability view
fn get_date_availability_keyboard(
    prefix: &String,
    availability_list: &Vec<AvailabilityDetails>,
    changes: &HashSet<Uuid>,
    role_type: &RoleType,
    start: usize,
    show: usize
) -> Result<InlineKeyboardMarkup, ()> {
    let slice_end = min(start + show, availability_list.len());
    let shown_entries = match availability_list.get(start..slice_end) {
        Some(entries) => entries,
        None => {
            log::error!("Cannot get availability entries slice");
            return Err(());
        }
    };

    let mut entries: Vec<Vec<InlineKeyboardButton>> = shown_entries
        .iter()
        .map(|entry| {
            // Check if the availability ID is in the changes vector
            let is_in_changes = changes.contains(&entry.id);
            let option_str = if entry.planned ^ is_in_changes {
                if entry.is_valid { "UNPLAN" } else { "UNPLAN (UNAVAIL)" }
            } else {
                "PLAN"
            };
            let truncated_remarks = if let Some(remarks) = &entry.remarks {
                if remarks.chars().count() > 8 {
                    format!(", {}...", remarks.chars().take(8).collect::<String>())
                } else {
                    format!(", {}", remarks)
                }
            } else {
                "".to_string()
            };

            let formatted = format!(
                "{} {}: {}{}",
                option_str,
                entry.ops_name,
                entry.ict_type.as_ref(),
                truncated_remarks
            );
            vec![InlineKeyboardButton::callback(
                formatted,
                AvailabilityCallbackData::PlanToggle { id: entry.id }.to_callback_data(prefix),
            )]
        })
        .collect();

    // Add role change buttons if applicable
    let change_view_roles: Vec<InlineKeyboardButton> = RoleType::iter()
        .filter_map(|role| {
            if *role_type != role {
                Some(InlineKeyboardButton::callback(
                    format!("VIEW {}", role.as_ref()),
                    AvailabilityCallbackData::ViewRole { role }.to_callback_data(prefix),
                ))
            } else {
                None
            }
        })
        .collect();

    // Add "PREV", "NEXT", and "DONE" buttons
    let mut pagination = Vec::new();
    if start > 0 {
        pagination.push(InlineKeyboardButton::callback("PREV", AvailabilityCallbackData::Prev.to_callback_data(prefix)));
    }
    if slice_end < availability_list.len() {
        pagination.push(InlineKeyboardButton::callback("NEXT", AvailabilityCallbackData::Next.to_callback_data(prefix)));
    }

    entries.push(change_view_roles);
    entries.push(pagination);
    entries.push(vec![
        InlineKeyboardButton::callback("DONE", AvailabilityCallbackData::Done.to_callback_data(prefix)),
        InlineKeyboardButton::callback("CANCEL", AvailabilityCallbackData::Cancel.to_callback_data(prefix))
    ]);

    Ok(InlineKeyboardMarkup::new(entries))
}

fn get_planned_change_text(availability: &AvailabilityDetails, changes: &HashSet<Uuid>) -> String {
    // Check if the availability ID is in the changes vector
    let is_in_changes = changes.contains(&availability.id);

    // Determine the current and toggled planned states
    let planned_str = if is_in_changes {
        // If the ID is in the changes, toggle the planned state for display
        if availability.planned {
            " PLANNED ➡️ UNPLANNED"
        } else {
            " UNPLANNED ➡️ PLANNED"
        }
    } else {
        // If no changes, display the actual planned state
        if availability.planned { " PLANNED" } else { "" }
    };

    planned_str.to_string()
}

// Generates the message text for user availability view
fn get_user_availability_text(
    user_details: &Usr,
    database_list: &Vec<AvailabilityDetails>,
    changes: &HashSet<Uuid>,
    start: usize,
    show: usize
) -> String {
    let mut message = String::new();
    if database_list.is_empty() {
        message.push_str("No upcoming availability\\.\n");
    } else {
        let usrtype_str = if database_list[0].usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
        // Pagination logic: slicing the list based on start and show
        let slice_end = std::cmp::min(start + show, database_list.len());
        let total_entries = database_list.len();

        // Add header with the range of entries being shown
        message.push_str(format!(
            "Showing availability for {}{}\\, entries {} to {} of {}:\n",
            utils::escape_special_characters(&user_details.ops_name),
            usrtype_str,
            start + 1,
            slice_end,
            total_entries
        ).as_str());

        let shown_entries = &database_list[start..slice_end];
        
        for availability in shown_entries {
            let date_str = utils::escape_special_characters(&availability.avail.format("%d %b, %Y").to_string());
            let ict_type_str = availability.ict_type.as_ref();

            // Determine the current and toggled planned states
            let planned_str = get_planned_change_text(&availability, changes);
            
            let avail_str = if availability.is_valid { "" } else { " *\\(UNAVAIL\\)*" };
            let saf100_str = if availability.saf100 { " SAF100 ISSUED" }
            else if availability.planned && availability.usr_type == UsrType::NS { " *PENDING SAF100*" }
            else { "" };

            // Truncate remarks to a max of 15 characters
            let remarks_str = if let Some(remarks) = &availability.remarks {
                if remarks.chars().count() > 15 {
                    format!("\nRemarks: {}\\.\\.\\.", utils::escape_special_characters(remarks.chars().take(15).collect::<String>().as_str()))
                } else {
                    format!("\nRemarks: {}", utils::escape_special_characters(&remarks))
                }
            } else {
                "".into()
            };

            message.push_str(&format!(
                "\\- {} __{}__\n{}{}\n{}{}\n",
                date_str,
                ict_type_str,
                avail_str,
                planned_str,
                saf100_str,
                remarks_str
            ));
        }

        // Add pagination information
        let current_page = (start / show) + 1;
        let total_pages = (total_entries as f64 / show as f64).ceil() as usize;
        message.push_str(&format!("\nPage {} of {}\n", current_page, total_pages));
    }

    message
}

// Generates the message text for date availability view
fn get_date_availability_text(
    selected_date: &NaiveDate,
    database_list: &Vec<AvailabilityDetails>,
    changes: &HashSet<Uuid>,
    start: usize,
    show: usize
) -> String {
    let date_str = utils::escape_special_characters(&selected_date.format("%d %b, %Y").to_string());
    let mut message = String::new();

    if database_list.is_empty() {
        message.push_str(format!("No users available on this date: {}\n", date_str).as_str());
    } else {
        // Pagination logic: slicing the list based on start and show
        let slice_end = std::cmp::min(start + show, database_list.len());
        let total_entries = database_list.len();
        
        message.push_str(format!(
            "Users available on {}\\, showing entries {} to {} of {}:\n",
            date_str,
            start + 1,
            slice_end,
            total_entries
        ).as_str());

        let shown_entries = &database_list[start..slice_end];
        
        for availability in shown_entries {
            let ict_type_str = availability.ict_type.as_ref();

            // Determine the current and toggled planned states
            let planned_str = get_planned_change_text(&availability, changes);
            
            let avail_str = if availability.is_valid { "" } else { " *\\(UNAVAIL\\)*" };
            let usrtype_str = if availability.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
            let saf100_str = if availability.saf100 { " SAF100 ISSUED" }
            else if availability.planned && availability.usr_type == UsrType::NS { " *PENDING SAF100*" }
            else { "" };

            // Truncate remarks to a max of 15 characters
            let remarks_str = if let Some(remarks) = &availability.remarks {
                if remarks.chars().count() > 15 {
                    format!("\nRemarks: {}\\.\\.\\.", utils::escape_special_characters(remarks.chars().take(15).collect::<String>().as_str()))
                } else {
                    format!("\nRemarks: {}", utils::escape_special_characters(&remarks))
                }
            } else {
                "".into()
            };

            message.push_str(&format!(
                "\\- {}{} __{}__\n{}{}\n{}{}\n",
                availability.ops_name, usrtype_str,
                ict_type_str,
                avail_str,
                planned_str,
                saf100_str,
                remarks_str
            ));
        }

        // Add pagination information
        let current_page = (start / show) + 1;
        let total_pages = (total_entries as f64 / show as f64).ceil() as usize;
        message.push_str(&format!("\nPage {} of {}\n", current_page, total_pages));
    }

    message
}

async fn display_enter_ops_name_or_date(bot: &Bot, chat_id: ChatId, username: &Option<String>) -> Option<MessageId> {
    send_msg(
        bot.send_message(chat_id, "Please enter an OPS NAME or a DATE:"),
        username,
    ).await
}

// Displays user availability with pagination using message editing
async fn display_user_availability(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    user_details: &Usr,
    database_list: &Vec<AvailabilityDetails>,
    changes: &HashSet<Uuid>,
    prefix: &String,
    start: usize,
    show: usize,
    msg_id: Option<MessageId>, // Optionally provide MessageId to edit
) -> Result<Option<MessageId>, ()> {
    // Generate the inline keyboard
    let markup = match get_user_availability_keyboard(prefix, database_list, changes, start, show) {
        Ok(kb) => kb,
        Err(_) => {
            send_msg(
                bot.send_message(chat_id, "Error encountered while getting availability."),
                username,
            ).await;
            return Err(());
        }
    };

    // Generate the message text
    let message_text = get_user_availability_text(user_details, database_list, changes, start, show);

    // Send or edit the message
    Ok(send_or_edit_msg(&bot, chat_id, username, msg_id, message_text, Some(markup), Some(ParseMode::MarkdownV2)).await)
}

// Displays date availability with pagination using message editing
async fn display_date_availability(
    bot: &Bot,
    chat_id: ChatId,
    username: &Option<String>,
    selected_date: &NaiveDate,
    database_list: &Vec<AvailabilityDetails>,
    changes: &HashSet<Uuid>,
    role_type: &RoleType,
    prefix: &String,
    start: usize,
    show: usize,
    msg_id: Option<MessageId>, // Optionally provide MessageId to edit
) -> Result<Option<MessageId>, ()> {
    // Generate the inline keyboard
    let markup = match get_date_availability_keyboard(prefix, database_list, changes, role_type, start, show) {
        Ok(kb) => kb,
        Err(_) => {
            send_msg(
                bot.send_message(chat_id, "Error encountered while getting availability."),
                username,
            ).await;
            return Err(());
        }
    };

    // Generate the message text
    let message_text = get_date_availability_text(selected_date, database_list, changes, start, show);
    
    // Send or edit message
    Ok(send_or_edit_msg(bot, chat_id, username, msg_id, message_text, Some(markup), Some(ParseMode::MarkdownV2)).await)
}

async fn handle_show_avail_by_user(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    user_details: Usr,
    availability_list: Vec<AvailabilityDetails>,
    changes: HashSet<Uuid>,
    role_type: RoleType,
    prefix: String,
    start: usize,
    show: usize,
) -> HandlerResult {
    // Viewing availability by user
    match display_user_availability(bot, dialogue.chat_id(), username, &user_details, &availability_list, &changes, &prefix, start, show, None, )
        .await {
        Ok(msg_id) => {
            match msg_id {
                None => dialogue.update(State::ErrorState).await?,
                Some(msg_id) => {
                    log::debug!("Transition to PlanView (viewing by user) with MsgId: {:?}, User: {:?}, Start: {:?}", msg_id, user_details, start);
                    dialogue.update(State::PlanView {
                        msg_id,
                        user_details: Some(user_details),
                        selected_date: None,
                        availability_list, changes,
                        role_type, prefix, start
                    }).await?
                }
            }
        }
        Err(_) => dialogue.update(State::ErrorState).await?,
    };

    Ok(())
}

async fn handle_show_avail_by_date(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    selected_date: NaiveDate,
    availability_list: Vec<AvailabilityDetails>,
    changes: HashSet<Uuid>,
    role_type: RoleType,
    prefix: String,
    start: usize,
    show: usize,
) -> HandlerResult {
    // Viewing availability by date
    match display_date_availability(bot, dialogue.chat_id(), username, &selected_date, &availability_list, &changes, &role_type, &prefix, start, show, None)
        .await {
        Ok(msg_id) => {
            match msg_id {
                None =>  dialogue.update(State::ErrorState).await?,
                Some(new_msg_id) => {
                    log::debug!("Transition to PlanView (viewing by date) with MsgId: {:?}, Date: {:?}, Start: {:?}", msg_id, selected_date, start);
                    dialogue.update(State::PlanView {
                        msg_id: new_msg_id,
                        user_details: None,
                        selected_date: Some(selected_date), availability_list, changes, 
                        role_type, prefix, start
                    }).await?
                }
            }
        }
        Err(_) => dialogue.update(State::ErrorState).await?,
    };

    Ok(())
}

// Handles updating the availability view (both user and date) based on the current state
async fn handle_re_show_options(
    bot: &Bot,
    dialogue: &MyDialogue,
    username: &Option<String>,
    user_details: Option<Usr>,
    selected_date: Option<NaiveDate>,
    changes: HashSet<Uuid>,
    role_type: RoleType,
    prefix: String,
    start: usize,
    show: usize,
    msg_id: MessageId, // Existing MessageId to edit
    pool: &PgPool
) -> HandlerResult {
    // Fetch the live availability details from the database
    let live_availability_list = match user_details {
        Some(ref user) => {
            controllers::scheduling::get_upcoming_availability_details_by_tele_id(pool, user.tele_id as u64).await
        }
        None => match selected_date {
            Some(ref date) => {
                controllers::scheduling::get_users_available_by_role_on_date(pool, date, &role_type).await
            }
            None => {
                dialogue.update(State::ErrorState).await?;
                return Ok(());
            }
        }
    };

    match live_availability_list {
        Ok(database_list) => {
            let newstart = if start < database_list.len()-1 { start } else { max(start as i64 - 8, 0) as usize };
            match user_details {
                Some(user_details) => {
                    // Viewing availability by user
                    match display_user_availability(
                        bot, dialogue.chat_id(), username,
                        &user_details,
                        &database_list,
                        &changes,
                        &prefix, start, show,
                        Some(msg_id),
                    ).await {
                        Ok(msg_id) => {
                            match msg_id {
                                None => dialogue.update(State::ErrorState).await?,
                                Some(new_msg_id) => {
                                    log::debug!("Transition to PlanView (viewing by user) with MsgId: {:?}, User: {:?}, Start: {:?}", msg_id, user_details, start);
                                    dialogue.update(State::PlanView {
                                        msg_id: new_msg_id,
                                        user_details: Some(user_details),
                                        selected_date: None,
                                        availability_list: database_list,
                                        changes, role_type, prefix, start: newstart
                                    }).await?
                                }
                            }
                        }
                        Err(_) => dialogue.update(State::ErrorState).await?,
                    };
                }
                None => {
                    // Viewing availability by date
                    if let Some(selected_date) = selected_date {
                        match display_date_availability(
                            bot, dialogue.chat_id(), username,
                            &selected_date,
                            &database_list,
                            &changes,
                            &role_type, &prefix, start, show,
                            Some(msg_id),
                        ).await {
                            Ok(msg_id) => {
                                match msg_id {
                                    None => {}
                                    Some(new_msg_id) => {
                                        log::debug!("Transition to PlanView (viewing by date) with MsgId: {:?}, Date: {:?}, Start: {:?}", msg_id, selected_date, start);
                                        dialogue.update(State::PlanView {
                                            msg_id: new_msg_id,
                                            user_details: None,
                                            selected_date: Some(selected_date),
                                            availability_list: database_list,
                                            changes,
                                            role_type, prefix, start: newstart
                                        }).await?;
                                    }
                                }
                            }
                            Err(_) => dialogue.update(State::ErrorState).await?,
                        };
                    } else {
                        dialogue.update(State::ErrorState).await?;
                    }
                }
            }
        }
        Err(_) => handle_error(bot, dialogue, dialogue.chat_id(), username).await
    }

    Ok(())
}

async fn display_retry_message(bot: &Bot, chat_id: ChatId, username: &Option<String>) {
    let retry_msg = "Please type an OPS NAME (to see availability for a user) or a DATE (to see availability for a date). Use /cancel to cancel current action, or use /user to show all users.";
    send_msg(
        bot.send_message(chat_id, retry_msg),
        username,
    ).await;
}

async fn handle_ops_name_or_date_input(bot: &Bot, dialogue: &MyDialogue, pool: &PgPool, user: &User, ops_name_or_date: String) -> HandlerResult {
    // Get the user in the database to determine their role
    let query_user_details = match controllers::user::get_user_by_tele_id(pool, user.id.0).await {
        Ok(user) => user,
        Err(_) => {
            handle_error(&bot, dialogue, dialogue.chat_id(), &user.username).await;
            return Ok(())
        },
    };
    // Generate random prefix to make the IDs only applicable to this dialogue instance
    let prefix = generate_prefix();
    // Try to interpret the argument as an OPS NAME first
    let cleaned_ops_name = ops_name_or_date.trim().to_uppercase();
    match controllers::user::user_exists_ops_name(&pool, cleaned_ops_name.as_ref()).await{
        Ok(exists) => {
            if exists {
                match controllers::user::get_user_by_ops_name(&pool, cleaned_ops_name.as_ref()).await {
                    Ok(user_details) => {
                        // show the dates for which the user is available
                        // Get the user's tele_id
                        let tele_id = user_details.tele_id as u64;

                        match controllers::scheduling::get_upcoming_availability_details_by_tele_id(&pool, tele_id).await {
                            Ok(availability_list) => {
                                // Display the user's availability
                                handle_show_avail_by_user(
                                    &bot, &dialogue, &user.username,
                                    user_details, availability_list, HashSet::new(),
                                    query_user_details.role_type,
                                    prefix, 0, 8,
                                ).await?;
                            }
                            Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                        }
                    }
                    Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                }
            } else {
                // unable to interpret as OPS NAME, interpreting as date
                match utils::parse_single_date(ops_name_or_date.as_ref()) {
                    Ok(selected_date) => {
                        // show the available users on that day
                        let today = chrono::Local::now().naive_local().date();
                        if selected_date < today {
                            send_msg(
                                bot.send_message(
                                    dialogue.chat_id(),
                                    "Please type a date that is today or in the future:",
                                ),
                                &user.username,
                            ).await;
                            dialogue.update(State::PlanSelect).await?;
                            return Ok(());
                        }
                        // Show the available users on that day
                        match controllers::scheduling::get_users_available_by_role_on_date(&pool, &selected_date, &query_user_details.role_type).await {
                            Ok(availability_list) => {
                                // Display the availability for the selected date
                                handle_show_avail_by_date(
                                    &bot, &dialogue, &user.username,
                                    selected_date, availability_list, HashSet::new(),
                                    query_user_details.role_type,
                                    prefix, 0, 8
                                ).await?;
                            }
                            Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
                        }
                    }
                    Err(_) => {
                        // Neither an OPS NAME nor a valid date
                        display_retry_message(bot, dialogue.chat_id(), &user.username).await;
                        dialogue.update(State::PlanSelect).await?;
                    }
                }
            }
        }
        Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &user.username).await
    }
    
    Ok(())
}

pub(super) async fn plan(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    ops_name_or_date: String,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "plan", "Command", msg);

    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::ErrorState).await?;
        return Ok(());
    };

    if !ops_name_or_date.is_empty() {
        handle_ops_name_or_date_input(&bot, &dialogue,&pool, &user, ops_name_or_date).await?;
    } else {
        display_enter_ops_name_or_date(&bot, dialogue.chat_id(), &user.username).await;
        dialogue.update(State::PlanSelect).await?;
    }

    Ok(())
}

pub(super) async fn plan_select(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "plan_select", "Message", msg);
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };
    
    match msg.text().map(ToOwned::to_owned) {
        Some(ops_name_or_date) => {
            handle_ops_name_or_date_input(&bot, &dialogue,&pool, &user, ops_name_or_date).await?;
        }
        None => {
            display_retry_message(&bot, dialogue.chat_id(), &user.username).await;
        }
    }
    
    Ok(())
}

pub(super) async fn plan_view(
    bot: Bot,
    dialogue: MyDialogue,
    q: CallbackQuery,
    (
        msg_id,
        user_details,
        selected_date,
        availability_list,
        mut changes,
        role_type,
        prefix,
        start
    ): (
        MessageId,
        Option<Usr>,
        Option<NaiveDate>,
        Vec<AvailabilityDetails>,
        HashSet<Uuid>,
        RoleType,
        String,
        usize
    ),
    pool: PgPool
) -> HandlerResult {
    log_endpoint_hit!(dialogue.chat_id(), "plan_view", "Callback", q,
        "MsgId" => msg_id,
        "UserDetails" => user_details,
        "SelectedDate" => selected_date,
        "AvailabilityList" => availability_list,
        "Changes" => changes,
        "RoleType" => role_type,
        "Prefix" => prefix,
        "Start" => start
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

    // Handle based on the variant
    match callback {
        AvailabilityCallbackData::Prev => {
            handle_re_show_options(
                &bot, &dialogue, &q.from.username,
                user_details, selected_date, changes, role_type,
                prefix, max(0, start as i64 - 8) as usize, 8,
                msg_id, &pool
            ).await?;
        }
        AvailabilityCallbackData::Next => {
            let entries_len = availability_list.len();
            handle_re_show_options(
                &bot, &dialogue, &q.from.username,
                user_details, selected_date, changes, role_type,
                prefix, if start+8 < entries_len { start+8 } else { start }, 8,
                msg_id, &pool
            ).await?;
        }
        AvailabilityCallbackData::Cancel => {
            send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), "Operation cancelled.".into(), None, None).await;
            dialogue.update(State::Start).await?;
        }
        AvailabilityCallbackData::Done => {
            // commit changes
            log_try_remove_markup(&bot, dialogue.chat_id(), msg_id).await;
            send_msg(
                bot.send_message(dialogue.chat_id(), "Done."),
                &q.from.username,
            ).await;

            match controllers::scheduling::toggle_planned_status_multiple(
                &pool,
                changes,
            ).await {
                Ok(availability_details) => {
                    let mut summary = String::new();

                    for details in &availability_details {
                        let user_type_suffix = if details.usr_type == UsrType::NS { " \\(NS\\)" } else { "" };
                        let status_message = if details.planned {
                            "has been planned"
                        } else {
                            "is no longer planned"
                        };
                        let formatted_avail = utils::escape_special_characters(&details.avail.format("%Y-%m-%d").to_string());

                        // Append to summary
                        summary.push_str(&format!(
                            "`{}`{} {} for {} on {}\n",
                            details.ops_name,
                            user_type_suffix,
                            status_message,
                            details.ict_type.as_ref(),
                            formatted_avail,
                        ));
                    }

                    // notify planned
                    notifier::emit::plan_notifications(
                        &bot,
                        format!(
                            "{} made the following changes:\n{}",
                            utils::username_link_tag(&q.from),
                            summary
                        ).as_str(),
                        &pool,
                        q.from.id.0 as i64
                    ).await;

                    send_or_edit_msg(&bot, dialogue.chat_id(), &q.from.username, Some(msg_id), summary, None, Some(ParseMode::MarkdownV2)).await;
                    dialogue.update(State::Start).await?;
                }
                Err(_) => handle_error(&bot, &dialogue, dialogue.chat_id(), &q.from.username).await
            }
        }
        AvailabilityCallbackData::PlanToggle { id: parsed_avail_uuid} => {
            // plan or unplan users
            // if currently planned -> unplan user
            // if currently unplanned -> plan user
            if changes.contains(&parsed_avail_uuid) {
                changes.remove(&parsed_avail_uuid);
            } else {
                changes.insert(parsed_avail_uuid);
            }

            handle_re_show_options(
                &bot, &dialogue, &q.from.username,
                user_details, selected_date, changes, role_type,
                prefix, start, 8, msg_id, &pool
            ).await?;
        }
        AvailabilityCallbackData::ViewRole { role: role_type_enum } => {
            match selected_date {
                Some(selected_date) => {
                    // Show the available users on that day
                    handle_re_show_options(
                        &bot, &dialogue, &q.from.username,
                        None,
                        Some(selected_date),
                        HashSet::new(), role_type_enum,
                        prefix, 0, 8, msg_id, &pool
                    ).await?;
                }
                None => {
                    send_msg(
                        bot.send_message(dialogue.chat_id(), "Invalid option."),
                        &q.from.username,
                    ).await;
                    handle_re_show_options(
                        &bot, &dialogue, &q.from.username,
                        user_details, selected_date, changes, role_type,
                        prefix, start, 8, msg_id, &pool
                    ).await?;
                }
            }
        }
        _ => {
            send_msg(
                bot.send_message(dialogue.chat_id(), "Invalid option."),
                &q.from.username,
            ).await;
        }
    }

    Ok(())
}