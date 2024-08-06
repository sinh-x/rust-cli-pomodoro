use chrono::Utc;
use clap_complete::generate;
use gluesql::prelude::{Glue, MemoryStorage};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::time::sleep;
use tokio::{net::UnixDatagram, sync::mpsc};
use tokio::{sync::mpsc::Sender, task::JoinHandle};

mod command;
mod database;
mod notification;
use database as db;
mod configuration;
mod error;
mod ipc;
mod line_handler;
mod logging;
mod report;
mod sled_databbase;

use crate::error::ConfigurationError;
use crate::ipc::{create_client_uds, create_server_uds, Bincodec, MessageRequest, MessageResponse};
use crate::notification::archived_notification;
use crate::notification::notify::{notify_break, notify_work};
use crate::notification::Notification;
use crate::sled_databbase::{NotificationSled, SledStore};
use crate::{
    command::{handler, util, CommandType},
    ipc::{get_uds_address, UdsType},
};
use crate::{
    configuration::{get_configuration, Configuration},
    ipc::UdsMessage,
};

#[macro_use]
extern crate log;

// key: notification id, value: spawned notification task
pub type TaskMap = HashMap<u16, JoinHandle<()>>;
pub type ArcGlue = Arc<Mutex<Glue<MemoryStorage>>>;
pub type ArcTaskMap = Arc<Mutex<TaskMap>>;

#[derive(Debug)]
pub struct UserInput {
    pub input: String,
    // pub oneshot_tx: oneshot::Sender<String>,
    pub source: InputSource,
}

#[derive(Debug)]
pub enum InputSource {
    StandardInput,
    UnixDomainSocket,
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(_) => {
            println!("Program completed successfully.");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            println!("Exiting the program due to an error.");
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    logging::initialize_logging();
    debug!("debug test, start pomodoro...");

    let path = Path::new("/home/sinh/.local/share/applications/sinh-x/pomodoro/sled_databbase");
    let sled_store = SledStore::new(&path).unwrap();

    let command_type = detect_command_type().await?;
    match command_type {
        CommandType::StartUp(config) => {
            info!("Starting server...");

            let glue = initialize_db().await;
            let mut id_manager: u16 = 1;
            let hash_map: Arc<Mutex<TaskMap>> = Arc::new(Mutex::new(HashMap::new()));
            let (user_input_tx, mut user_input_rx) = mpsc::channel::<UserInput>(64);

            // Start handling stdin input in a separate task
            let stdin_tx = user_input_tx.clone();
            let stdin_tx = user_input_tx.clone();
            let _input_handle = match line_handler::handle(stdin_tx) {
                Ok(handle) => Some(handle),
                Err(_) => None, // Ignore errors from stdin
            };
            // Start handling UDS input
            let uds_input_tx = user_input_tx.clone();
            let server_uds_option = create_server_uds().await.unwrap();
            let server_tx = match server_uds_option {
                Some(uds) => {
                    let server_uds = Arc::new(uds);
                    let (server_rx, server_tx) = (server_uds.clone(), server_uds.clone());
                    let _uds_input_handle =
                        spawn_uds_input_handler(uds_input_tx, server_tx, server_rx);
                    Some(server_uds)
                }
                None => {
                    error!("main:Failed to create or connect to server UDS");
                    std::process::exit(1);
                }
            };

            // Main loop to handle user input
            debug!("Main loop to handle user input");
            loop {
                debug!("Server is alive");
                while let Some(user_input) = user_input_rx.recv().await {
                    debug!("Server is alive inside while");
                    match handle_user_input(
                        user_input,
                        &mut id_manager,
                        &hash_map,
                        &glue,
                        &config,
                        &server_tx,
                        &sled_store,
                    )
                    .await
                    {
                        Ok(_) => {}
                        Err(e) => debug!("There was an error handling the user input: {}", e),
                    }
                }
            }
        }
        CommandType::UdsClient(matches) => {
            debug!("CommandType::UdsClient");
            let socket = create_client_uds().await?;
            handler::uds_client::handle(matches, socket).await?;
        }
        CommandType::AutoComplete(sub_matches) => {
            if sub_matches.contains_id("shell") {
                if let Some(shell) = util::parse_shell(&sub_matches) {
                    let mut main_command = command::get_main_command();
                    let bin_name = main_command.get_name().to_string();
                    let mut stdout = std::io::stdout();
                    generate(shell, &mut main_command, bin_name, &mut stdout);
                }
            } else {
                println!("No shell name was passed");
            }
        }
    }

    debug!("handle_uds_client_command called successfully");

    Ok(())
}

async fn handle_user_input(
    user_input: UserInput,
    id_manager: &mut u16,
    hash_map: &Arc<Mutex<TaskMap>>,
    glue: &ArcGlue,
    config: &Arc<Configuration>,
    server_tx: &Option<Arc<UnixDatagram>>,
    sled_store: &SledStore,
) -> Result<(), Box<dyn Error>> {
    let input = user_input.input.as_str();
    debug!("Input: {:?}", input);

    match handler::user_input::handle(input, id_manager, hash_map, glue, config, sled_store).await {
        Ok(mut output) => match user_input.source {
            InputSource::StandardInput => {}
            InputSource::UnixDomainSocket => {
                if let Some(ref server_tx) = server_tx {
                    let client_addr = get_uds_address(UdsType::Client);
                    ipc::send_to(
                        server_tx,
                        client_addr,
                        MessageResponse::new(output.take_body())
                            .encode()?
                            .as_slice(),
                    )
                    .await;
                }
            }
        },
        Err(e) => {
            debug!("There was an error analyzing the input: {}", e);
            if let Some(ref server_tx) = server_tx {
                let client_addr = get_uds_address(UdsType::Client);
                if let Ok(encoded) = MessageResponse::new(vec![format!(
                    "There was an error analyzing the input: {}",
                    e
                )])
                .encode()
                {
                    let _ = ipc::send_to(server_tx, client_addr, encoded.as_slice()).await;
                } else {
                    debug!("Error encoding message response");
                }
            }
        }
    }

    debug!("Handled input: {:?}", user_input);
    util::print_start_up();

    Ok(())
}

async fn detect_command_type() -> Result<CommandType, ConfigurationError> {
    let matches = command::get_start_and_uds_client_command().get_matches();
    debug!("handle_uds_client_command, matches: {:?}", &matches);

    let command_type = match matches.subcommand().is_none() {
        true => CommandType::StartUp(get_configuration(&matches)?),
        false => {
            if let Some(val) = matches.subcommand_matches("completion") {
                CommandType::AutoComplete(val.to_owned())
            } else {
                CommandType::UdsClient(matches)
            }
        }
    };

    Ok(command_type)
}

async fn initialize_db() -> Arc<Mutex<Glue<MemoryStorage>>> {
    let glue = Arc::new(Mutex::new(db::get_memory_glue()));
    db::initialize(glue.clone()).await;

    glue
}

// TODO(young): refactor and move to proper place
pub fn spawn_notification(
    configuration: Arc<Configuration>,
    hash_map: Arc<Mutex<TaskMap>>,
    glue: ArcGlue,
    notification: Notification,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (id, _, work_time, break_time, _, _, _) = notification.get_values();
        debug!("id: {}, task started", id);

        let before_start_remaining = (notification.get_start_at() - Utc::now()).num_seconds();
        let before = tokio::time::Duration::from_secs(before_start_remaining as u64);
        debug!("before_start_remaining: {:?}", before_start_remaining);
        sleep(before).await;

        if work_time > 0 {
            let wt = tokio::time::Duration::from_secs(work_time as u64 * 60);
            sleep(wt).await;
            debug!("id ({}), work time ({}) done", id, work_time);

            // TODO(young): handle notify report err
            let result = notify_work(&configuration).await;
            if let Ok(report) = result {
                info!("\n{}", report);
                println!("Notification report generated");
                util::write_output(&mut io::stdout());
            }
        }

        if break_time > 0 {
            let bt = tokio::time::Duration::from_secs(break_time as u64 * 60);
            sleep(bt).await;
            debug!("id ({}), break time ({}) done", id, break_time);

            // TODO(young): handle notify report err
            let result = notify_break(&configuration).await;
            if let Ok(report) = result {
                info!("\n{}", report);
                println!("Notification report generated");
                util::write_output(&mut io::stdout());
            }
        }

        let result = notification::delete_notification(id, hash_map, glue.clone()).await;
        if result.is_err() {
            trace!("error occurred while deleting notification");
        }

        debug!("id: {}, notification work time done!", id);
    })
}

fn spawn_uds_input_handler(
    uds_tx: Sender<UserInput>,
    server_tx: Arc<UnixDatagram>,
    server_rx: Arc<UnixDatagram>,
) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
    tokio::spawn(async move {
        let rx = server_rx;
        let mut buf = vec![0u8; 256];
        debug!("rx is initialized successfully");
        loop {
            debug!("inside unix domain socket task");
            // TODO(young) handle result
            let (size, addr) = rx.recv_from(&mut buf).await.unwrap();
            debug!("size: {:?}, addr: {:?}", size, addr);

            if let Some(path) = addr.as_pathname() {
                // ignore request from unnamed address
                if path != get_uds_address(ipc::UdsType::Client).as_path() {
                    debug!("addr is different");
                    continue;
                }
            }

            let uds_message = UdsMessage::decode(&buf[..size]).unwrap();
            match uds_message {
                UdsMessage::Public(message) => {
                    let user_input: UserInput = MessageRequest::into(message);
                    debug!("user_input: {:?}", user_input);

                    // Line 296
                    if let Err(e) = uds_tx.send(user_input).await {
                        eprintln!("Error sending user input: {}", e);
                        return Err(e.into());
                    }
                }
                UdsMessage::Internal(message) => {
                    debug!("internal_message ok, {:?}", message);
                    match message {
                        ipc::internal::Message::Ping => {
                            ipc::send_to(
                                &server_tx,
                                addr.as_pathname().unwrap().to_path_buf(),
                                ipc::internal::Message::Pong.encode().unwrap().as_slice(),
                            )
                            .await;
                        }
                        ipc::internal::Message::Pong => {}
                    }
                }
            }
        }
        Ok(())
    })
}
