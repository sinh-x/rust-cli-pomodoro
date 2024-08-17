use chrono::Utc;
use clap::error::ErrorKind;
use clap::{ArgMatches, Command};
use std::process;
use std::result;
use std::str::SplitWhitespace;
use std::sync::Arc;
use tabled::locator::ByColumnName;
use tabled::object::Segment;
use tabled::style::HorizontalLine;
use tabled::Modify;
use tabled::{Alignment, Disable};
use tabled::{Style, TableIteratorExt};

use crate::command::output::{OutputAccumulater, OutputType};
use crate::command::{self, action::ActionType};
use crate::error::UserInputHandlerError;
use crate::notification::get_new_notification_sled;
use crate::notification::notify::notify_work;
use crate::SledStore;
use crate::{configuration::Configuration, ArcGlue};
use crate::{spawn_notification, ArcTaskMap};

type HandleUserInputResult = result::Result<(), UserInputHandlerError>;

pub async fn handle(
    user_input: &str,
    notification_task_map: &ArcTaskMap,
    glue: &ArcGlue,
    configuration: &Arc<Configuration>,
    sled_store: &SledStore,
) -> Result<OutputAccumulater, UserInputHandlerError> {
    let command = command::get_main_command();
    let input = user_input.split_whitespace();
    let mut output_accumulator = OutputAccumulater::new();

    debug!("input: {:?}", input);

    let matches = match get_matches(command, input, &mut output_accumulator)? {
        Some(args) => args,
        None => {
            return Ok(output_accumulator);
        }
    };

    let (action_type, sub_matches) = matches
        .subcommand()
        .ok_or(UserInputHandlerError::NoSubcommand)
        .and_then(|(s, sub_matches)| {
            ActionType::parse(s)
                .map(|s| (s, sub_matches))
                .map_err(UserInputHandlerError::ParseError)
        })?;

    match action_type {
        ActionType::Create => {
            handle_create(
                sub_matches,
                configuration,
                notification_task_map,
                &mut output_accumulator,
                sled_store,
            )
            .await?;
        }
        ActionType::Queue => {
            handle_queue(
                sub_matches,
                configuration,
                notification_task_map,
                &mut output_accumulator,
                &sled_store,
            )
            .await?;
        }
        ActionType::Delete => {}
        ActionType::List => handle_list(sub_matches, &mut output_accumulator, sled_store).await?,
        ActionType::Test => handle_test(configuration, &mut output_accumulator).await?,
        ActionType::History => {
            handle_history(sub_matches, glue, &mut output_accumulator, sled_store).await?
        }
        ActionType::Exit => process::exit(0),
        ActionType::Clear => print!("\x1B[2J\x1B[1;1H"),
    }

    Ok(output_accumulator)
}

async fn handle_create(
    matches: &ArgMatches,
    configuration: &Arc<Configuration>,
    notification_task_map: &ArcTaskMap,
    output_accumulator: &mut OutputAccumulater,
    sled_store: &SledStore,
) -> HandleUserInputResult {
    let notification_new = get_new_notification_sled(matches, Utc::now(), configuration.clone())
        .map_err(UserInputHandlerError::NotificationError)?;

    let _ = sled_store.create_notification(&notification_new);
    let id = notification_new.get_id();

    let handle = spawn_notification(
        configuration.clone(),
        notification_task_map.clone(),
        &sled_store,
        notification_new,
    );

    notification_task_map.lock().unwrap().insert(id, handle);
    output_accumulator.push(
        OutputType::Println,
        format!(
            "[{}] Notification (id: {}) created",
            chrono::offset::Local::now(),
            id
        ),
    );

    Ok(())
}

async fn handle_queue(
    matches: &ArgMatches,
    configuration: &Arc<Configuration>,
    notification_task_map: &ArcTaskMap,
    output_accumulator: &mut OutputAccumulater,
    sled_store: &SledStore,
) -> HandleUserInputResult {
    let created_at = sled_store.get_time_for_queue_notification()?;
    let notification_new = get_new_notification_sled(matches, created_at, configuration.clone())
        .map_err(UserInputHandlerError::NotificationError)?;

    let id = notification_new.get_id();
    let _ = sled_store.create_notification(&notification_new);
    debug!("Queue notification: {:?}", notification_new);

    notification_task_map.lock().unwrap().insert(
        id,
        spawn_notification(
            configuration.clone(),
            notification_task_map.clone(),
            sled_store,
            notification_new,
        ),
    );
    output_accumulator.push(
        OutputType::Println,
        format!(
            "[{}] Notification (id: {}) created and queued",
            chrono::offset::Local::now(),
            id
        ),
    );

    Ok(())
}

async fn handle_test(
    configuration: &Arc<Configuration>,
    output_accumulator: &mut OutputAccumulater,
) -> HandleUserInputResult {
    debug!("Message:NotificationTest called!");
    let report = notify_work(&configuration.clone())
        .await
        .map_err(UserInputHandlerError::NotificationError)?;
    output_accumulator.push(OutputType::Info, format!("\n{}", report));

    debug!("Message:NotificationTest done");
    output_accumulator.push(
        OutputType::Println,
        String::from("Notification Test called"),
    );

    Ok(())
}

async fn handle_list(
    sub_matches: &ArgMatches,
    output_accumulator: &mut OutputAccumulater,
    sled_store: &SledStore,
) -> HandleUserInputResult {
    debug!("handle_list::List called!");

    let mut main_table_sled = match sled_store.list_notifications() {
        Ok(sleds) => sleds.table(),
        Err(e) => {
            output_accumulator.push(OutputType::Error, format!("Error: {}", e));
            return Ok(());
        }
    };

    let styled_table = main_table_sled
        .with(
            Style::modern()
                .off_horizontal()
                .horizontals([HorizontalLine::new(1, Style::modern().get_horizontal())]),
        )
        .with(Modify::new(Segment::all()).with(Alignment::center()));

    let table_sled: String = if !sub_matches.get_flag("percentage") {
        styled_table
            .with(Disable::column(ByColumnName::new("percentage")))
            .to_string()
    } else {
        styled_table.to_string()
    };

    output_accumulator.push(OutputType::Info, format!("\n{}", table_sled));
    output_accumulator.push(OutputType::Println, String::from("List succeed"));

    Ok(())
}

async fn handle_history(
    _sub_matches: &ArgMatches,
    _glue: &ArcGlue,
    output_accumulator: &mut OutputAccumulater,
    sled_store: &SledStore,
) -> HandleUserInputResult {
    debug!("Message:History called!");
    debug!("Message:History done!");

    let mut main_table_sled = match sled_store.list_all_notifications() {
        Ok(sleds) => {
            let item_count = sleds.len();
            debug!("History: sled items count {}", item_count);
            sleds.table()
        }
        Err(e) => {
            output_accumulator.push(OutputType::Error, format!("Error: {}", e));
            return Ok(());
        }
    };

    let table_sled = main_table_sled
        .with(
            Style::modern()
                .off_horizontal()
                .horizontals([HorizontalLine::new(1, Style::modern().get_horizontal())]),
        )
        .with(Modify::new(Segment::all()).with(Alignment::center()))
        .to_string();

    output_accumulator.push(OutputType::Info, format!("\n{}", table_sled));
    output_accumulator.push(OutputType::Println, String::from("History succeed"));

    Ok(())
}

// get_matches extract ArgMatches from input string
fn get_matches(
    command: Command,
    input: SplitWhitespace,
    output_accumulator: &mut OutputAccumulater,
) -> Result<Option<ArgMatches>, UserInputHandlerError> {
    match command.try_get_matches_from(input) {
        Ok(args) => Ok(Some(args)),
        Err(err) => {
            match err.kind() {
                // DisplayHelp has help message in error
                ErrorKind::DisplayHelp => {
                    // print!("\n{}\n", err);
                    // TODO(young): test format! works well
                    output_accumulator
                        .push(OutputType::Print, format!("\n{}\n", err.render().ansi()));
                    Ok(None)
                }
                // clap automatically print version string with out newline.
                ErrorKind::DisplayVersion => {
                    output_accumulator.push(OutputType::Println, String::from(""));
                    Ok(None)
                }
                _ => Err(UserInputHandlerError::CommandMatchError(err)),
            }
        }
    }
}
