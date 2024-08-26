use chrono::prelude::*;
use clap_complete::generate;
use gluesql::prelude::{Glue, MemoryStorage};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
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
use crate::notification::notify::{notify_break, notify_work};
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
pub type TaskMap = HashMap<uuid::Uuid, JoinHandle<()>>;
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

    debug!("debug test, start pomodoro... 1.5.4");
    let command_type = detect_command_type().await?;
    match command_type {
        CommandType::StartUp(config) => {
            info!("Starting server...");

            let path =
                Path::new("/home/sinh/.local/share/applications/sinh-x/pomodoro/sled_databbase");
            if let Some(parent_path) = path.parent() {
                fs::create_dir_all(parent_path)?;
            } else {
                panic!(
                    "could not create parent directory for sled database: {}",
                    path.display()
                )
            }
            let sled_store = SledStore::new(&path).unwrap();

            let glue = initialize_db().await;
            let hash_map: Arc<Mutex<TaskMap>> = Arc::new(Mutex::new(HashMap::new()));
            let (user_input_tx, mut user_input_rx) = mpsc::channel::<UserInput>(64);

            // Start handling stdin input in a separate task
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

            match sled_store.list_notifications() {
                Ok(active_notifications) => {
                    for current_notification in active_notifications {
                        hash_map.lock().unwrap().insert(
                            current_notification.get_id(),
                            spawn_notification(
                                config.clone(),
                                hash_map.clone(),
                                &sled_store,
                                current_notification,
                            ),
                        );
                    }
                }
                Err(e) => {
                    debug!("There was an error spawn existing notifications: {}", e)
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
    hash_map: &Arc<Mutex<TaskMap>>,
    glue: &ArcGlue,
    config: &Arc<Configuration>,
    server_tx: &Option<Arc<UnixDatagram>>,
    sled_store: &SledStore,
) -> Result<(), Box<dyn Error>> {
    let input = user_input.input.as_str();
    debug!("Input: {:?}", input);

    match handler::user_input::handle(input, hash_map, glue, config, sled_store).await {
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
    _hash_map: Arc<Mutex<TaskMap>>,
    _sled_store: &SledStore,
    notification: NotificationSled,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (id, _, work_time, break_time, _, _, _) = notification.get_values();
        let notify_time_min = -10;
        let notify_time_max = 10;

        if work_time > 0 {
            let duration = notification.work_expired_at - Utc::now();
            let wt = duration.num_seconds().max(0) as u64;
            match wt {
                0 => debug!("spawn_notification: id ({}) no wait for work!", id),
                1.. => {
                    debug!(
                        "spawn_notification: id ({}), work time ({}) sleep {:?} secs",
                        id, work_time, wt
                    );
                    sleep(tokio::time::Duration::from_secs(wt)).await;
                    let time_diff = notification.work_expired_at - Utc::now(); // TODO(young): handle notify report err
                    let time_diff = time_diff.num_seconds();
                    if time_diff >= notify_time_min && time_diff <= notify_time_max {
                        let result = notify_work(&configuration).await;
                        if let Ok(report) = result {
                            info!("\n{}", report);
                            debug!("spawn_notification: Notification report generated");
                            util::write_output(&mut io::stdout());
                        }
                    }
                }
            }
            debug!("spawn_notification: id ({}) work time done!", id);
        }

        if break_time > 0 {
            let duration = notification.break_expired_at - Utc::now();
            let bt = duration.num_seconds().max(0) as u64;
            match bt {
                0 => debug!("spawn_notification: id ({}) no wait for break!", id),
                1.. => {
                    debug!(
                        "spawn_notification: id ({}), break time ({}) sleep {:?} secs",
                        id, break_time, bt
                    );
                    sleep(tokio::time::Duration::from_secs(bt)).await;
                    let time_diff = notification.break_expired_at - Utc::now(); // TODO(young): handle notify report err
                    let time_diff = time_diff.num_seconds();
                    if time_diff >= notify_time_min && time_diff <= notify_time_max {
                        // TODO(young): handle notify report err
                        let result = notify_break(&configuration).await;
                        if let Ok(report) = result {
                            info!("\n{}", report);
                            debug!("spawn_notification: Notification report generated");
                            util::write_output(&mut io::stdout());
                        }
                    }
                }
            }
            debug!("spawn_notification: id ({}) break time done!", id);
        }
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
    })
}
