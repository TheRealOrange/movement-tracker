use std::collections::HashSet;
use super::commands::{cancel, help, set_menu_buttons, Commands, PrivilegedCommands};
use super::{send_msg, HandlerResult, MyDialogue};
use crate::bot::apply::{apply_edit_admin, apply_edit_name, apply_edit_ops_name, apply_edit_prompt, apply_edit_role, apply_edit_type, apply_view, approve};
use chrono::NaiveDate;
use sqlx::PgPool;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::dispatching::{dialogue, UpdateHandler};
use teloxide::dptree::{case, endpoint};
use teloxide::prelude::*;
use teloxide::types::{ChatKind, MessageId, ReplyParameters};
use uuid::Uuid;
use super::register::{register, register_complete, register_name, register_ops_name, register_role, register_type};
use super::user::{user, user_edit_admin, user_edit_delete, user_edit_name, user_edit_ops_name, user_edit_prompt, user_edit_role, user_edit_type, user_select};
use crate::bot::availability::{availability, availability_add_callback, availability_add_change_type, availability_add_complete, availability_add_message, availability_add_remarks, availability_delete_confirm, availability_modify, availability_modify_remarks, availability_modify_type, availability_select, availability_view, AvailabilityAction};
use crate::bot::forecast::{forecast, forecast_view};
use crate::bot::plan::{plan, plan_select, plan_view};
use crate::types::{Apply, Availability, AvailabilityDetails, Ict, NotificationSettings, RoleType, Usr, UsrType};
use crate::{controllers, log_endpoint_hit};
use crate::bot::notify::{notify, notify_settings};
use crate::bot::saf100::{saf100, saf100_confirm, saf100_select, saf100_view};
use crate::bot::upcoming::upcoming;

#[derive(Clone, Default)]
pub(super) enum State {
    #[default]
    Start,
    // States used for registering for an account
    RegisterRole {
        msg_id: MessageId,
        prefix: String,
    },
    RegisterType {
        msg_id: MessageId,
        prefix: String,
        role_type: RoleType,
    },
    RegisterName {
        msg_id: MessageId,
        role_type: RoleType,
        user_type: UsrType,
    },
    RegisterOpsName {
        msg_id: MessageId,
        role_type: RoleType,
        user_type: UsrType,
        name: String,
    },
    RegisterComplete {
        msg_id: MessageId,
        prefix: String,
        role_type: RoleType,
        user_type: UsrType,
        name: String,
        ops_name: String,
    },
    // States used for looking through and approving applications
    ApplyView {
        msg_id: MessageId,
        applications: Vec<Apply>,
        prefix: String,
        start: usize
    },
    ApplyEditPrompt {
        msg_id: MessageId,
        prefix: String,
        application: Apply,
        admin: bool
    },
    ApplyEditName {
        msg_id: MessageId,
        change_msg_id: MessageId,
        application: Apply,
        admin: bool
    },
    ApplyEditOpsName {
        msg_id: MessageId,
        change_msg_id: MessageId,
        application: Apply,
        admin: bool
    },
    ApplyEditRole {
        msg_id: MessageId,
        prefix: String,
        change_msg_id: MessageId,
        application: Apply,
        admin: bool
    },
    ApplyEditType {
        msg_id: MessageId,
        prefix: String,
        change_msg_id: MessageId,
        application: Apply,
        admin: bool
    },
    ApplyEditAdmin {
        msg_id: MessageId,
        prefix: String,
        change_msg_id: MessageId,
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
        msg_id: MessageId,
        prefix: String,
        availability_list: Vec<Availability>
    },
    AvailabilitySelect {
        msg_id: MessageId,
        availability_list: Vec<Availability>,
        prefix: String,
        start: usize
    },
    AvailabilityModify {
        msg_id: MessageId,
        prefix: String,
        availability_entry: Availability,
        action: AvailabilityAction,
        start: usize
    },
    AvailabilityModifyType {
        msg_id: MessageId,
        prefix: String,
        change_msg_id: MessageId,
        availability_entry: Availability,
        action: AvailabilityAction,
        start: usize
    },
    AvailabilityModifyRemarks {
        msg_id: MessageId,
        change_msg_id: MessageId,
        availability_entry: Availability,
        action: AvailabilityAction,
        start: usize
    },
    AvailabilityDeleteConfirm {
        msg_id: MessageId,
        prefix: String,
        availability_entry: Availability,
        action: AvailabilityAction,
        start: usize
    },
    AvailabilityAdd {
        msg_id: MessageId,
        prefix: String,
        avail_type: Ict
    },
    AvailabilityAddChangeType {
        msg_id: MessageId,
        prefix: String,
        change_type_msg_id: MessageId,
        avail_type: Ict
    },
    AvailabilityAddRemarks {
        msg_id: MessageId,
        prefix: String,
        change_msg_id: MessageId,
        avail_type: Ict,
        avail_dates: Vec<NaiveDate>
    },
    // States meant for viewing the forecast
    ForecastView {
        msg_id: MessageId,
        prefix: String,
        availability_list: Vec<AvailabilityDetails>,
        role_type: RoleType, 
        start: NaiveDate, 
        end: NaiveDate
    },
    // States meant for planning SANS for flight
    PlanSelect,
    PlanView {
        msg_id: MessageId,
        user_details: Option<Usr>,
        selected_date: Option<NaiveDate>,
        availability_list: Vec<AvailabilityDetails>,
        changes: HashSet<Uuid>,
        role_type: RoleType,
        prefix: String,
        start: usize
    },
    // TODO: States meant for SANS attendance confirmation
    // States meant for editing users
    UserSelect,
    UserEdit {
        msg_id: MessageId,
        user_details: Usr,
        prefix: String
    },
    UserEditName {
        msg_id: MessageId,
        change_msg_id: MessageId,
        user_details: Usr,
        prefix: String
    },
    UserEditOpsName {
        msg_id: MessageId,
        change_msg_id: MessageId,
        user_details: Usr,
        prefix: String
    },
    UserEditRole {
        msg_id: MessageId,
        change_msg_id: MessageId,
        user_details: Usr,
        prefix: String
    },
    UserEditType {
        msg_id: MessageId,
        change_msg_id: MessageId,
        user_details: Usr,
        prefix: String
    },
    UserEditAdmin {
        msg_id: MessageId,
        change_msg_id: MessageId,
        user_details: Usr,
        prefix: String
    },
    UserEditDeleteConfirm {
        msg_id: MessageId,
        change_msg_id: MessageId,
        user_details: Usr,
        prefix: String
    },
    // States meant for tracking saf100 issued
    Saf100Select {
        msg_id: MessageId,
        prefix: String
    },
    Saf100View {
        msg_id: MessageId,
        availability_list: Vec<AvailabilityDetails>,
        prefix: String,
        start: usize,
        action: String
    },
    Saf100Confirm {
        msg_id: MessageId,
        availability: Availability,
        prefix: String,
        start: usize,
        action: String
    },
    // States meant for editing the notification settings
    NotifySettings {
        notification_settings: NotificationSettings,
        chat_id: ChatId,
        prefix: String,
        msg_id: MessageId
    },
    ErrorState
}

pub(super) fn schema() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    // Privileged Command Handler
    let admin_command_handler = teloxide::filter_command::<PrivilegedCommands, _>()
        .branch(case![PrivilegedCommands::User { ops_name }].branch(dptree::filter_async(check_private).endpoint(user)))
        .branch(case![PrivilegedCommands::Approve].branch(dptree::filter_async(check_private).endpoint(approve)))
        .branch(case![PrivilegedCommands::Plan { ops_name_or_date }].branch(dptree::filter_async(check_private).endpoint(plan)))
        .branch(case![PrivilegedCommands::SAF100].branch(dptree::filter_async(check_private).endpoint(saf100)))
        .branch(case![PrivilegedCommands::Notify].endpoint(notify));

    // Public Commands: Accessible to All Users (excluding /cancel)
    let public_commands = teloxide::filter_command::<Commands, _>()
        .branch(case![Commands::Start].endpoint(help))
        .branch(case![Commands::Help].endpoint(help))
        .branch(case![Commands::Register].branch(dptree::filter_async(check_private).endpoint(register)));

    // Registered Commands: Accessible Only to Registered Users
    let registered_commands = teloxide::filter_command::<Commands, _>()
        .branch(dptree::filter_async(check_registered)
            .branch(case![Commands::Forecast].endpoint(forecast))
            .branch(case![Commands::Availability].branch(dptree::filter_async(check_private).endpoint(availability)))
            .branch(case![Commands::Upcoming].branch(dptree::filter_async(check_private).endpoint(upcoming)))
        );

    // Combine Public and Registered Commands
    let command_handler = public_commands.branch(registered_commands);

    // Define the /cancel Command Handler
    let cancel_handler = teloxide::filter_command::<Commands, _>()
        .branch(case![Commands::Cancel].endpoint(cancel));

    let message_handler = Update::filter_message()
        .branch(case![State::ErrorState].endpoint(error_state))
        .branch(command_handler)
        .branch(cancel_handler)
        .branch(case![State::RegisterName { msg_id, role_type, user_type }].endpoint(register_name))
        .branch(case![State::RegisterOpsName { msg_id, role_type, user_type, name }].endpoint(register_ops_name))
        .branch(dptree::filter_async(check_admin)
            .branch(admin_command_handler)
            .branch(case![State::ApplyEditName { msg_id, change_msg_id, application, admin }].endpoint(apply_edit_name))
            .branch(case![State::ApplyEditOpsName { msg_id, change_msg_id, application, admin }].endpoint(apply_edit_ops_name))
            .branch(case![State::UserEditName { msg_id, change_msg_id, user_details, prefix }].endpoint(user_edit_name))
            .branch(case![State::UserEditOpsName { msg_id, change_msg_id, user_details, prefix }].endpoint(user_edit_ops_name))
            .branch(case![State::PlanSelect].endpoint(plan_select))
            .branch(case![State::UserSelect].endpoint(user_select))
        )
        .branch(case![State::AvailabilityModifyRemarks { msg_id, change_msg_id, availability_entry, action, start }].endpoint(availability_modify_remarks))
        .branch(case![State::AvailabilityAdd { msg_id, prefix, avail_type }].endpoint(availability_add_message))
        .branch(case![State::AvailabilityAddRemarks { msg_id, prefix, change_msg_id, avail_type, avail_dates }].endpoint(availability_add_remarks))
        //everything below is a catchall case to tell the user they should use a callback button rather than send a message
        .branch(case![State::RegisterRole { msg_id, prefix }].endpoint(press_button_prompt))
        .branch(case![State::RegisterType { msg_id, prefix, role_type }].endpoint(press_button_prompt))
        .branch(case![State::RegisterComplete { msg_id, prefix, role_type, user_type, name, ops_name }].endpoint(press_button_prompt))
        .branch(case![State::NotifySettings { notification_settings, chat_id, prefix, msg_id }].endpoint(press_button_prompt))
        .branch(case![State::ApplyView { msg_id, applications, prefix, start }].endpoint(press_button_prompt))
        .branch(case![State::ApplyEditPrompt { msg_id, prefix, application, admin }].endpoint(press_button_prompt))
        .branch(case![State::ApplyEditRole { msg_id, prefix, change_msg_id, application, admin }].endpoint(press_button_prompt))
        .branch(case![State::ApplyEditType { msg_id, prefix, change_msg_id, application, admin }].endpoint(press_button_prompt))
        .branch(case![State::ApplyEditAdmin { msg_id, prefix, change_msg_id, application, admin }].endpoint(press_button_prompt))
        .branch(case![State::UserEdit { msg_id, user_details, prefix }].endpoint(press_button_prompt))
        .branch(case![State::UserEditRole { msg_id, change_msg_id, user_details, prefix }].endpoint(press_button_prompt))
        .branch(case![State::UserEditType { msg_id, change_msg_id, user_details, prefix }].endpoint(press_button_prompt))
        .branch(case![State::UserEditAdmin { msg_id, change_msg_id, user_details, prefix }].endpoint(press_button_prompt))
        .branch(case![State::UserEditDeleteConfirm { msg_id, change_msg_id, user_details, prefix }].endpoint(press_button_prompt))
        .branch(case![State::PlanView { msg_id, user_details, selected_date, availability_list, changes, role_type, prefix, start }].endpoint(press_button_prompt))
        .branch(case![State::Saf100Select { msg_id, prefix }].endpoint(press_button_prompt))
        .branch(case![State::Saf100View { msg_id, availability_list, prefix, start, action }].endpoint(press_button_prompt))
        .branch(case![State::Saf100Confirm { msg_id, availability, prefix, start, action }].endpoint(press_button_prompt))
        .branch(case![State::AvailabilityView { msg_id, prefix, availability_list }].endpoint(press_button_prompt))
        .branch(case![State::AvailabilitySelect { msg_id, availability_list, prefix, start }].endpoint(press_button_prompt))
        .branch(case![State::AvailabilityModify { msg_id, prefix, availability_entry, action, start }].endpoint(press_button_prompt))
        .branch(case![State::AvailabilityModifyType { msg_id, prefix, change_msg_id, availability_entry, action, start }].endpoint(press_button_prompt))
        .branch(case![State::AvailabilityAddChangeType { msg_id, prefix, change_type_msg_id, avail_type }].endpoint(press_button_prompt))
        .branch(case![State::AvailabilityDeleteConfirm { msg_id, prefix, availability_entry, action, start }].endpoint(press_button_prompt))
        .branch(case![State::ForecastView { msg_id, prefix, availability_list, role_type, start, end }].endpoint(press_button_prompt));
    

    let callback_query_handler = Update::filter_callback_query()
        .branch(case![State::RegisterRole { msg_id, prefix }].endpoint(register_role))
        .branch(case![State::RegisterType { msg_id, prefix, role_type }].endpoint(register_type))
        .branch(case![State::RegisterComplete { msg_id, prefix, role_type, user_type, name, ops_name }].endpoint(register_complete))
        .branch(dptree::filter_async(check_admin_callback)
            .branch(case![State::NotifySettings { notification_settings, chat_id, prefix, msg_id }].endpoint(notify_settings))
            .branch(case![State::ApplyView { msg_id, applications, prefix, start }].endpoint(apply_view))
            .branch(case![State::ApplyEditPrompt { msg_id, prefix, application, admin }].endpoint(apply_edit_prompt))
            .branch(case![State::ApplyEditRole { msg_id, prefix, change_msg_id, application, admin }].endpoint(apply_edit_role))
            .branch(case![State::ApplyEditType { msg_id, prefix, change_msg_id, application, admin }].endpoint(apply_edit_type))
            .branch(case![State::ApplyEditAdmin { msg_id, prefix, change_msg_id, application, admin }].endpoint(apply_edit_admin))
            .branch(case![State::UserEdit { msg_id, user_details, prefix }].endpoint(user_edit_prompt))
            .branch(case![State::UserEditRole { msg_id, change_msg_id, user_details, prefix }].endpoint(user_edit_role))
            .branch(case![State::UserEditType { msg_id, change_msg_id, user_details, prefix }].endpoint(user_edit_type))
            .branch(case![State::UserEditAdmin { msg_id, change_msg_id, user_details, prefix }].endpoint(user_edit_admin))
            .branch(case![State::UserEditDeleteConfirm { msg_id, change_msg_id, user_details, prefix }].endpoint(user_edit_delete))
            .branch(case![State::PlanView { msg_id, user_details, selected_date, availability_list, changes, role_type, prefix, start }].endpoint(plan_view))
            .branch(case![State::Saf100Select { msg_id, prefix }].endpoint(saf100_select))
            .branch(case![State::Saf100View { msg_id, availability_list, prefix, start, action }].endpoint(saf100_view))
            .branch(case![State::Saf100Confirm { msg_id, availability, prefix, start, action }].endpoint(saf100_confirm))
        )
        .branch(case![State::AvailabilityView { msg_id, prefix, availability_list }].endpoint(availability_view))
        .branch(case![State::AvailabilitySelect { msg_id, availability_list, prefix, start }].endpoint(availability_select))
        .branch(case![State::AvailabilityModify { msg_id, prefix, availability_entry, action, start }].endpoint(availability_modify))
        .branch(case![State::AvailabilityModifyType { msg_id, prefix, change_msg_id, availability_entry, action, start }].endpoint(availability_modify_type))
        .branch(case![State::AvailabilityAdd { msg_id, prefix, avail_type }].endpoint(availability_add_callback))
        .branch(case![State::AvailabilityAddChangeType { msg_id, prefix, change_type_msg_id, avail_type }].endpoint(availability_add_change_type))
        .branch(case![State::AvailabilityAddRemarks { msg_id, prefix, change_msg_id, avail_type, avail_dates }].endpoint(availability_add_complete))
        .branch(case![State::AvailabilityDeleteConfirm { msg_id, prefix, availability_entry, action, start }].endpoint(availability_delete_confirm))
        .branch(case![State::ForecastView { msg_id, prefix, availability_list, role_type, start, end }].endpoint(forecast_view));

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
        .branch(endpoint(invalid_state))
}

async fn press_button_prompt(bot: Bot, msg: Message, dialogue: MyDialogue) -> HandlerResult {
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        log::error!("Cannot get user from message");
        dialogue.update(State::Start).await?;
        return Ok(());
    };
    
    send_msg(
        bot.send_message(user.id, "Please press a button, or type /cancel to abort."),
        &(user.username)
    ).await;
    
    Ok(())
}

async fn check_private(bot: Bot, msg: Message) -> bool {
    // Early return if the message has no sender (msg.from() is None)
    let user = if let Some(ref user) = msg.from {
        user
    } else {
        return false;
    };

    // Determine the kind of chat the message was sent in
    match &msg.chat.kind {
        ChatKind::Private(_) => true, // Private chat
        ChatKind::Public(_) => {
            send_msg(
                bot.send_message(user.id, format!("Please send {} as a private message only", msg.text().unwrap_or("your message"))),
                &(user.username)
            ).await;
            false
        }, // Group, Supergroup, or Channel
    }
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

async fn check_admin(bot: Bot, dialogue: MyDialogue, msg: Message, pool: PgPool) -> bool {
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
                    // Set menu buttons based on the user's admin status
                    // Determine the kind of chat the message was sent in
                    let is_public_chat = matches!(&msg.chat.kind, ChatKind::Public(_));
                    set_menu_buttons(bot, dialogue.chat_id(), user.id, retrieved_usr.admin, is_public_chat).await;
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
    let chat_id = dialogue.chat_id();

    // Declare a variable to hold the log message
    let mut log_message = format!("Reached error state for chat_id: {}", chat_id);

    // Attempt to retrieve user details using getChat
    match bot.get_chat(chat_id).await {
        Ok(chat) => {
            // Append user details to the log message if retrieval is successful
            log_message.push_str(&format!(
                ", username: {:?}, first_name: {}, last_name: {:?}",
                chat.username().unwrap_or("<no username>"),
                chat.first_name().unwrap_or("<no first name>"),
                chat.last_name().unwrap_or("<no last name>")
            ));
        }
        Err(e) => {
            // Append the error message if retrieval fails
            log_message.push_str(&format!(", failed to retrieve user details: {}", e));
        }
    }

    // Log the complete message in one log entry
    log::info!("{}", log_message);

    // Log the endpoint hit
    log_endpoint_hit!(chat_id, "error_state");

    // Send an error message to the user
    send_msg(
        bot.send_message(chat_id, "Error occurred, returning to start!"),
        &None
    ).await;

    // Update the dialogue state to the start
    dialogue.update(State::Start).await?;

    Ok(())
}

async fn invalid_state(dialogue: MyDialogue) -> HandlerResult {
    log::info!("Reached an invalid state! (how did you do this?)");
    log_endpoint_hit!(dialogue.chat_id(), "invalid_state");
    dialogue.update(State::Start).await?;
    Ok(())
}