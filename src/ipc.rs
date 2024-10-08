use bincode::error::DecodeError;
use bincode::error::EncodeError;
use bincode::Decode;
use bincode::Encode;
use std::env;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::UnixDatagram;
use tokio::time::timeout;

use crate::command::action::ActionType;
use crate::InputSource;
use crate::UserInput;

const SOCKET_SERVER_ADDR: &str = "rust-cli-pomodoro-server.sock";
const SOCKET_CLIENT_ADDR: &str = "rust-cli-pomodoro-client.sock";

const CHUNK: usize = 2048;

#[derive(Debug)]
pub enum UdsType {
    Server,
    Client,
}

#[derive(bincode::Encode, bincode::Decode, PartialEq, Debug, Eq)]
pub enum UdsMessage {
    Public(MessageRequest),
    Internal(internal::Message),
}

impl Bincodec for UdsMessage {
    type Message = Self;
}

pub trait Bincodec {
    type Message;

    fn encode(self) -> Result<Vec<u8>, EncodeError>
    where
        Self: Sized,
        Self: Encode,
    {
        let vec = bincode::encode_to_vec(self, bincode::config::standard())?;

        Ok(vec)
    }

    fn decode(byte: &[u8]) -> Result<Self::Message, DecodeError>
    where
        <Self as Bincodec>::Message: Decode,
    {
        let (message, _): (Self::Message, usize) =
            bincode::decode_from_slice(byte, bincode::config::standard())?;

        Ok(message)
    }
}

#[derive(bincode::Encode, bincode::Decode, PartialEq, Debug, Eq)]
pub enum MessageRequest {
    Create {
        work: Option<u16>,
        r#break: Option<u16>,
        desciption: Option<String>,
    },
    Queue {
        work: Option<u16>,
        r#break: Option<u16>,
        desciption: Option<String>,
    },
    Delete {
        id: u16,
        all: bool,
    },
    List {
        show_percentage: bool,
    },
    Test,
    History {
        should_clear: bool,
    },
}

impl Bincodec for MessageRequest {
    type Message = Self;
}

impl From<MessageRequest> for UserInput {
    fn from(request: MessageRequest) -> Self {
        let input = match request {
            MessageRequest::Create {
                work,
                r#break,
                //TODO: implement description
                desciption: _,
            } => {
                let mut data = format!("{} ", String::from(ActionType::Create));

                if let Some(val) = work {
                    data.push_str(&format!("-w {} ", val))
                }

                if let Some(val) = r#break {
                    data.push_str(&format!("-b {}", val))
                }

                data
            }
            MessageRequest::Queue {
                work,
                r#break,
                //TODO: implement description
                desciption: _,
            } => {
                let mut data = format!("{} ", String::from(ActionType::Queue));

                if let Some(val) = work {
                    data.push_str(&format!("-w {} ", val))
                }

                if let Some(val) = r#break {
                    data.push_str(&format!("-b {}", val))
                }

                data
            }
            MessageRequest::Delete { id, all } => {
                if all {
                    format!("{} -a", String::from(ActionType::Delete))
                } else {
                    format!("{} -i {}", String::from(ActionType::Delete), id)
                }
            }
            MessageRequest::List { show_percentage } => {
                if show_percentage {
                    format!("{} -p", String::from(ActionType::List))
                } else {
                    String::from(ActionType::List)
                }
            }
            MessageRequest::Test => String::from(ActionType::Test),
            MessageRequest::History { should_clear } => {
                if should_clear {
                    format!("{} --clear", String::from(ActionType::History))
                } else {
                    String::from(ActionType::History)
                }
            }
        };

        debug!("input: {:?}", input);

        UserInput {
            input,
            source: InputSource::UnixDomainSocket,
        }
    }
}

#[derive(bincode::Encode, bincode::Decode, PartialEq, Debug, Eq)]
pub struct MessageResponse {
    body: Vec<String>,
}

impl MessageResponse {
    pub fn new(body: Vec<String>) -> Self {
        MessageResponse { body }
    }

    pub fn get_body(&self) -> &Vec<String> {
        &self.body
    }

    pub fn print(self) {
        self.get_body().iter().for_each(|m| println!("{}", m));
    }
}

impl Bincodec for MessageResponse {
    type Message = Self;
}

pub mod internal {
    use bincode;
    use bincode::error::{DecodeError, EncodeError};
    use tokio::net::UnixDatagram;

    use crate::command::handler::uds_client::BUFFER_LENGTH;

    #[derive(bincode::Encode, bincode::Decode, PartialEq, Debug, Eq)]
    pub enum Message {
        Ping,
        Pong,
    }

    impl Message {
        pub fn encode(self) -> Result<Vec<u8>, EncodeError> {
            let vec = bincode::encode_to_vec(self, bincode::config::standard())?;

            Ok(vec)
        }

        pub fn decode(byte: &[u8]) -> Result<Self, DecodeError> {
            let (msg, _): (Message, usize) =
                bincode::decode_from_slice(byte, bincode::config::standard())?;

            Ok(msg)
        }
    }

    pub async fn decode_from_socket(
        socket: UnixDatagram,
    ) -> Result<Message, Box<dyn std::error::Error>> {
        let mut vec = Vec::new();
        let mut total_size = 0;

        loop {
            let mut buf = vec![0u8; BUFFER_LENGTH];
            let (size, _) = socket.recv_from(&mut buf).await?;

            let dgram = &buf[..size];
            debug!("dgram len: {:?}", dgram.len());
            vec.extend_from_slice(dgram);
            debug!("vec length: {:?}", vec.len());

            total_size += size;

            if size == 0 {
                break;
            }
        }

        debug!("total_size: {}", total_size);
        let dgram = &vec.as_slice()[..total_size];

        let message = Message::decode(dgram)?;

        Ok(message)
    }
}

// TODO(young): The result should be optional
pub async fn create_server_uds() -> Result<Option<UnixDatagram>, std::io::Error> {
    debug!("create_server_uds called begin");
    let result = detect_address_in_use().await;
    debug!("result: {:?}", result);
    if let Ok(address_in_use) = result {
        if address_in_use {
            debug!("address_in_use");
            return Ok(None);
        }
    }

    let server_addr = match create_uds_address(UdsType::Server, false) {
        Ok(addr) => addr,
        Err(e) => {
            debug!("create_server_uds called okay");
            if e.kind() == std::io::ErrorKind::Other && e.to_string() == "Server is already running"
            {
                error!("Server is already running");
                return Err(e);
            } else {
                if e.kind() == ErrorKind::ConnectionRefused {
                    // Server is not running, create a new server with should_remove set to true
                    debug!(
                        "Server is not running, create a new server with should_remove set to true"
                    );
                    match create_uds_address(UdsType::Server, true) {
                        Ok(addr) => addr,
                        Err(e) => {
                            error!("An error occurred when creating the UDS address: {}", e);
                            return Err(e);
                        }
                    }
                } else {
                    // An error occurred
                    error!("An error occurred: {}", e);
                    return Err(e);
                }
            }
        }
    };

    let socket = match UnixDatagram::bind(server_addr) {
        Ok(socket) => socket,
        Err(e) => {
            error!("An error occurred when binding the socket: {}", e);
            return Err(e);
        }
    };

    debug!("create_server_uds called");
    Ok(Some(socket))
}

pub async fn create_client_uds() -> Result<UnixDatagram, std::io::Error> {
    let server_addr = get_uds_address(UdsType::Server);
    let client_addr = create_uds_address(UdsType::Client, true)?;

    let socket = UnixDatagram::bind(client_addr)?;
    socket.connect(server_addr)?;

    debug!("create_client_uds called");
    Ok(socket)
}

async fn detect_address_in_use() -> Result<bool, std::io::Error> {
    debug!("detect_address_in_use called");
    let socket = create_client_uds().await?;

    // TODO(young): Force `send` must get UdsMessage type
    let timeout_result = timeout(
        Duration::from_millis(500),
        socket.send(
            UdsMessage::Internal(internal::Message::Ping)
                .encode()
                .unwrap()
                .as_slice(),
        ),
    )
    .await;
    if let Err(err) = timeout_result {
        debug!("did not send value within 500 ms, {:?}", err);
    }

    let timeout_result = timeout(
        Duration::from_millis(500),
        internal::decode_from_socket(socket),
    )
    .await;
    match timeout_result {
        Ok(message_result) => {
            debug!("message_result: {:?}", message_result);
            if let Ok(msg) = message_result {
                if msg == internal::Message::Pong {
                    return Ok(true);
                }
            }
        }
        Err(err) => {
            debug!("did not receive value within 500 ms, {:?}", err);
            return Ok(false);
        }
    }

    Ok(false)
}

fn create_uds_address(r#type: UdsType, should_remove: bool) -> std::io::Result<PathBuf> {
    debug!("create_uds_address: {:?} - {}", &r#type, &should_remove);

    let path = get_uds_address(r#type);

    if path.exists() {
        if should_remove {
            debug!("path {:?} exists, remove it before binding", &path);
            fs::remove_file(&path)?;
        } else {
            match UnixDatagram::unbound() {
                Ok(datagram) => match datagram.connect(&path) {
                    Ok(_) => {
                        error!("Server is running at path {:?}", &path);
                        return Err(Error::new(ErrorKind::Other, "Server is already running"));
                    }
                    Err(e) => {
                        error!("An error occurred when creating the UDS address: {}", e);
                        return Err(e);
                    }
                },
                Err(e) => {
                    error!("An error occurred: {}", e);
                    return Err(e);
                }
            }
        }
    }
    debug!("create_uds_address, path: {:?}", path);

    Ok(path)
}

pub fn get_uds_address(r#type: UdsType) -> PathBuf {
    let socket_addr = match r#type {
        UdsType::Server => SOCKET_SERVER_ADDR,
        UdsType::Client => SOCKET_CLIENT_ADDR,
    };

    let mut p = env::temp_dir();
    let username = env::var("USER").unwrap_or("default".to_string());
    p.push(username); // Use the username to create a sub-directory
    std::fs::create_dir_all(&p).expect("Failed to create directory");
    p.push(socket_addr);

    debug!("get_uds_address, path: {:?}", p);
    p
}

pub async fn send_to(socket: &UnixDatagram, target: PathBuf, buf: &[u8]) {
    let size = buf.len();
    debug!("buf length: {}", size);
    debug!("size / CHUNK: {}", size / CHUNK);

    for i in 0..size / CHUNK + 1 {
        let (start, end) = (CHUNK * i, CHUNK * (i + 1));
        let end = if end > size { size } else { end };

        let buf = &buf[start..end];
        debug!("buf length to be sent: {}", buf.len());
        match socket.send_to(buf, &target).await {
            Ok(_) => debug!("sent for loop"),
            Err(e) => error!("send_to error: {:?}", e),
        };

        // Wait for certain time due to
        // "No buffer space available" error in mac os.
        #[cfg(target_os = "macos")]
        sleep(tokio::time::Duration::from_millis(1)).await;
    }

    let fin = Vec::new();
    match socket.send_to(fin.as_slice(), &target).await {
        Ok(_) => debug! {"sent final"},
        Err(e) => error!("There was an error sending to the socket: {}", e),
    }
}
