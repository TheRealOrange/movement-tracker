use super::commands::{cancel, help, Commands, PrivilegedCommands};
use super::{send_msg, HandlerResult, MyDialogue};
use crate::bot::apply::{apply_edit_admin, apply_edit_name, apply_edit_ops_name, apply_edit_prompt, apply_edit_role, apply_edit_type, apply_view, approve};
use chrono::NaiveDate;
use sqlx::PgPool;
use teloxide::dispatching::{dialogue, UpdateHandler};
use teloxide::dptree::{case, endpoint};
use teloxide::types::ReplyParameters;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

use super::register::{register, register_complete, register_name, register_ops_name, register_role, register_type};
use super::user::{user, user_edit_admin, user_edit_delete, user_edit_name, user_edit_ops_name, user_edit_prompt, user_edit_type};
use crate::bot::availability::{availability, availability_add_callback, availability_add_change_type, availability_add_complete, availability_add_message, availability_add_remarks, availability_modify, availability_modify_remarks, availability_modify_type, availability_select, availability_view};
use crate::bot::forecast::{forecast, forecast_view};
use crate::bot::plan::{plan, plan_view};
use crate::types::{Apply, Availability, AvailabilityDetails, Ict, RoleType, Usr, UsrType};
use crate::{controllers, log_endpoint_hit};

#[derive(Clone, Default)]
pub(super) enum State {
    #[default]
    Start,
    // States used for registering for an account
    RegisterRole,
    RegisterType {
        role_type: RoleType,
    },
    RegisterName {
        role_type: RoleType,
        user_type: UsrType,
    },
    RegisterOpsName {
        role_type: RoleType,
        user_type: UsrType,
        name: String,
    },
    RegisterComplete {
        role_type: RoleType,
        user_type: UsrType,
        name: String,
        ops_name: String,
    },
    // States used for looking through and approving applications
    ApplyView {
        applications: Vec<Apply>,
        prefix: String,
        start: usize
    },
    ApplyEditPrompt {
        application: Apply,
        admin: bool
    },
    ApplyEditName {
        application: Apply,
        admin: bool
    },
    ApplyEditOpsName {
        application: Apply,
        admin: bool
    },
    ApplyEditRole {
        application: Apply,
        admin: bool
    },
    ApplyEditType {
        application: Apply,
        admin: bool
    },
    ApplyEditAdmin {
        application: Apply,
        admin: bool
    },
    // TODO: States used for adding personal movement details 
    // MovementView,
    // EditMovement,
    // AddMovement,
    // AddMovementDetails {
    //     details: String,
    // },
    // States used for adding and modifying availability for SANS
    AvailabilityView {
        availability_list: Vec<Availability>
    },
    AvailabilitySelect {
        availability_list: Vec<Availability>,
        action: String,
        prefix: String,
        start: usize
    },
    AvailabilityModify {
        availability_entry: Availability,
        availability_list: Vec<Availability>,
        action: String,
        prefix: String,
        start: usize
    },
    AvailabilityModifyType {
        availability_entry: Availability,
        action: String,
        start: usize
    },
    AvailabilityModifyRemarks {
        availability_entry: Availability,
        action: String,
        start: usize
    },
    AvailabilityAdd {
        avail_type: Ict
    },
    AvailabilityAddChangeType {
        avail_type: Ict
    },
    AvailabilityAddRemarks {
        avail_type: Ict,
        avail_dates: Vec<NaiveDate>
    },
    // States meant for viewing the forecast
    ForecastView {
        availability_list: Vec<AvailabilityDetails>,
        role_type: RoleType, 
        start: NaiveDate, 
        end: NaiveDate
    },
    // States meant for planning SANS for flight
    PlanView {
        user_details: Option<Usr>,
        selected_date: Option<NaiveDate>,
        availability_list: Vec<AvailabilityDetails>,
        role_type: RoleType,
        prefix: String,
        start: usize
    },
    // TODO: States meant for SANS attendance confirmation
    // States meant for editing users
    UserEdit {
        user_details: Usr
    },
    UserEditName {
        user_details: Usr
    },
    UserEditOpsName {
        user_details: Usr
    },
    UserEditType {
        user_details: Usr
    },
    UserEditAdmin {
        user_details: Usr
    },
    UserEditDeleteConfirm {
        user_details: Usr
    },
    ErrorState
}

pub(super) fn schema() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    let command_handler = teloxide::filter_command::<Commands, _>()
        .branch(case![Commands::Help].endpoint(help))
        .branch(case![Commands::Register].endpoint(register))
        .branch(dptree::filter_async(check_registered)
            .branch(case![Commands::Availability].endpoint(availability))
            .branch(case![Commands::Forecast].endpoint(forecast))
        )
        .branch(case![Commands::Cancel].endpoint(cancel));

    let admin_command_handler = teloxide::filter_command::<PrivilegedCommands, _>()
        .branch(case![PrivilegedCommands::User { ops_name }].endpoint(user))
        .branch(case![PrivilegedCommands::Approve].endpoint(approve))
        .branch(case![PrivilegedCommands::Plan { ops_name_or_date }].endpoint(plan));

    let message_handler = Update::filter_message()
        .branch(case![State::ErrorState].endpoint(error_state))
        .branch(command_handler)
        .branch(case![State::RegisterName { role_type, user_type }].endpoint(register_name))
        .branch(case![State::RegisterOpsName { role_type, user_type, name }].endpoint(register_ops_name))
        .branch(dptree::filter_async(check_registered)
            .branch(dptree::filter_async(check_admin)
                .branch(admin_command_handler)
                .branch(case![State::ApplyEditName { application, admin }].endpoint(apply_edit_name))
                .branch(case![State::ApplyEditOpsName { application, admin }].endpoint(apply_edit_ops_name))
                .branch(case![State::UserEditName { user_details }].endpoint(user_edit_name))
                .branch(case![State::UserEditOpsName { user_details }].endpoint(user_edit_ops_name))
            )
            .branch(case![State::AvailabilityView { availability_list }].endpoint(availability_view))
            .branch(case![State::AvailabilityModifyRemarks { availability_entry, action, start }].endpoint(availability_modify_remarks))
            .branch(case![State::AvailabilityAdd { avail_type }].endpoint(availability_add_message))
            .branch(case![State::AvailabilityAddRemarks { avail_type, avail_dates }].endpoint(availability_add_remarks))
        );
    

    let callback_query_handler = Update::filter_callback_query()
        .branch(case![State::RegisterRole].endpoint(register_role))
        .branch(case![State::RegisterType { role_type }].endpoint(register_type))
        .branch(case![State::RegisterComplete { role_type, user_type, name, ops_name }].endpoint(register_complete))
        .branch(dptree::filter_async(check_admin_callback)
            .branch(case![State::ApplyView { applications, prefix, start }].endpoint(apply_view))
            .branch(case![State::ApplyEditPrompt { application, admin }].endpoint(apply_edit_prompt))
            .branch(case![State::ApplyEditRole { application, admin }].endpoint(apply_edit_role))
            .branch(case![State::ApplyEditType { application, admin }].endpoint(apply_edit_type))
            .branch(case![State::ApplyEditAdmin { application, admin }].endpoint(apply_edit_admin))
            .branch(case![State::UserEdit { user_details }].endpoint(user_edit_prompt))
            .branch(case![State::UserEditType { user_details }].endpoint(user_edit_type))
            .branch(case![State::UserEditAdmin { user_details }].endpoint(user_edit_admin))
            .branch(case![State::UserEditDeleteConfirm { user_details }].endpoint(user_edit_delete))
            .branch(case![State::PlanView { user_details, selected_date, availability_list, role_type, prefix, start }].endpoint(plan_view))
        )
        .branch(case![State::AvailabilityView { availability_list }].endpoint(availability_view))
        .branch(case![State::AvailabilitySelect { availability_list, action, prefix, start }].endpoint(availability_select))
        .branch(case![State::AvailabilityModify { availability_entry, availability_list, action, prefix, start }].endpoint(availability_modify))
        .branch(case![State::AvailabilityModifyType { availability_entry, action, start }].endpoint(availability_modify_type))
        .branch(case![State::AvailabilityAdd { avail_type }].endpoint(availability_add_callback))
        .branch(case![State::AvailabilityAddChangeType { avail_type }].endpoint(availability_add_change_type))
        .branch(case![State::AvailabilityAddRemarks { avail_type, avail_dates }].endpoint(availability_add_complete))
        .branch(case![State::ForecastView { availability_list, role_type, start, end }].endpoint(forecast_view));

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
        .branch(endpoint(invalid_state))
}

// Function to handle "server is overloaded" message if connection acquisition fails
async fn reply_server_overloaded(bot: &Bot, msg: &Message) {
    send_msg(
        bot.send_message(msg.chat.id, "Server is overloaded. Please try again later.")
            .reply_parameters(ReplyParameters::new(msg.id)),
        match msg.from {
            Some(ref from) => &(from.username),
            _ => &None
        }
    ).await;
}

async fn check_registered(bot: Bot, msg: Message, pool: PgPool) -> bool {
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        return false;
    };

    // Helper function to log and return false on any errors
    let handle_error = || async {
        send_msg(
            bot.send_message(msg.chat.id, "Error occurred accessing the database")
                .reply_parameters(ReplyParameters::new(msg.id)),
            &(user.username)
        ).await;
        false
    };

    // Check if the telegram ID exists in the database
    match controllers::user::user_exists_tele_id(&pool, user.id.0).await{
        Ok(true) => true,
        Ok(false) => {
            // Check if the user is an admin
            match controllers::apply::apply_exists_tele_id(&pool, user.id.0).await {
                Ok(exists) => {
                    if exists == true {
                        send_msg(
                            bot.send_message(msg.chat.id, "You have a pending registration. Please wait for it to be approved."),
                            &(user.username)
                        ).await;
                    } else {
                        send_msg(
                            bot.send_message(msg.chat.id, "Please submit a registration with the bot using /register and wait for approval"),
                            &(user.username)
                        ).await;
                    }
                    false
                }
                Err(_) => handle_error().await,
            }
        },
        Err(_) => handle_error().await
    }
}

async fn check_admin(bot: Bot, msg: Message, pool: PgPool) -> bool {
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(user) = msg.from {
        user
    } else {
        return false;
    };

    // Helper function to log and return false on any errors
    let handle_error = || async {
        send_msg(
            bot.send_message(msg.chat.id, "Error occurred accessing the database")
                .reply_parameters(ReplyParameters::new(msg.id)),
            &(user.username)
        ).await;
        false
    };

    // Check if the telegram ID exists in the database
    match controllers::user::user_exists_tele_id(&pool, user.id.0).await{
        Ok(true) => {
            // Check if the user is an admin
            match controllers::user::get_user_by_tele_id(&pool, user.id.0).await {
                Ok(retrieved_usr) => {
                    retrieved_usr.admin
                }
                Err(_) => handle_error().await,
            }
        },
        Ok(false) => false,
        Err(_) => handle_error().await
    }
}

async fn check_admin_callback(bot: Bot, dialogue: MyDialogue, q: CallbackQuery, pool: PgPool) -> bool {
    // Helper function to log and return false on any errors
    let handle_error = || async {
        send_msg(
            bot.send_message(dialogue.chat_id(), "Error occurred accessing the database"),
            &q.from.username
        ).await;
        false
    };

    // Check if the telegram ID exists in the database
    match controllers::user::user_exists_tele_id(&pool, q.from.id.0).await{
        Ok(true) => {
            // Check if the user is an admin
            match controllers::user::get_user_by_tele_id(&pool, q.from.id.0).await {
                Ok(retrieved_usr) => {
                    retrieved_usr.admin
                }
                Err(_) => handle_error().await,
            }
        },
        Ok(false) => false,
        Err(_) => handle_error().await
    }
}

async fn error_state(bot: Bot, dialogue: MyDialogue) -> HandlerResult {
    log::info!("Reached error state");
    log_endpoint_hit!(dialogue.chat_id(), "error_state");
    send_msg(
        bot.send_message(dialogue.chat_id(), "Error occurred, returning to start!"),
        &None
    ).await;
    dialogue.update(State::Start).await?;
    Ok(())
}

async fn invalid_state(dialogue: MyDialogue) -> HandlerResult {
    log::info!("Reached an invalid state! (how did you do this?)");
    log_endpoint_hit!(dialogue.chat_id(), "invalid_state");
    dialogue.update(State::Start).await?;
    Ok(())
}