use bincode::error::{DecodeError, EncodeError};
use notify_rust::error::Error as NotifyRustError;
use reqwest::Error as RequestError;
use serde_json::error::Error as SerdeJsonError;
use std::{error::Error, fmt, io, result};

pub type NotifyResult = result::Result<(), NotificationError>;

// TODO(young): Replace main error type to this
enum _PomodoroError {
    Notification,
    Configuration,
    UdsHandler,
    UserInputHandler,
    Parse,
}

// notification error enum
#[derive(Debug)]
pub enum NotificationError {
    // TODO(Desktop also need NotifyRustError type???)
    Desktop(NotifyRustError),
    Slack(RequestError),
    Discord(RequestError),
    EmptyConfiguration,
    NewNotification(ParseError),
    EmptyTimeValues,
}

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NotificationError::Desktop(_) => write!(f, "NotificationError::Desktop"),
            NotificationError::Slack(_) => write!(f, "NotificationError::Slack"),
            NotificationError::Discord(_) => write!(f, "NotificationError::Discord"),
            NotificationError::EmptyConfiguration => write!(f, "configuration is empty"),
            NotificationError::NewNotification(e) => {
                write!(f, "failed to get new notification: {}", e)
            }
            NotificationError::EmptyTimeValues => write!(
                f,
                "Cannot create a notification with 0 work time and 0 break time"
            ),
        }
    }
}

impl std::error::Error for NotificationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NotificationError::Desktop(ref e) => Some(e),
            NotificationError::Slack(ref e) => Some(e),
            NotificationError::Discord(ref e) => Some(e),
            NotificationError::EmptyConfiguration => None,
            NotificationError::NewNotification(ref e) => Some(e),
            NotificationError::EmptyTimeValues => None,
        }
    }
}

#[derive(Debug)]
pub enum ConfigurationError {
    FileNotFound,
    FileOpenError(io::Error),
    JsonError(SerdeJsonError),
    SlackConfigNotFound,
    DiscordConfigNotFound,
    UnspecifiedWorkTime,
    UnspecifiedBreakTime,
    LoadFail(io::Error),
    // config json wrong format?
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigurationError::FileNotFound => write!(f, "can not find configuration file"),
            ConfigurationError::FileOpenError(_) => write!(f, "failed to open the file"),
            ConfigurationError::JsonError(_) => write!(f, "failed to deserialize json"),
            ConfigurationError::SlackConfigNotFound => {
                write!(f, "can not find slack config in json")
            }
            ConfigurationError::DiscordConfigNotFound => {
                write!(f, "can not find discord config in json")
            }
            ConfigurationError::UnspecifiedWorkTime => write!(f, "not specified"),
            ConfigurationError::UnspecifiedBreakTime => write!(f, "not specified"),
            ConfigurationError::LoadFail(e) => write!(f, "failed to load: {}", e),
        }
    }
}

impl std::error::Error for ConfigurationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigurationError::FileNotFound => None,
            ConfigurationError::FileOpenError(ref e) => Some(e),
            ConfigurationError::JsonError(ref e) => Some(e),
            ConfigurationError::SlackConfigNotFound => None,
            ConfigurationError::DiscordConfigNotFound => None,
            ConfigurationError::UnspecifiedWorkTime => None,
            ConfigurationError::UnspecifiedBreakTime => None,
            ConfigurationError::LoadFail(ref e) => Some(e),
        }
    }
}

#[derive(Debug)]
pub enum UdsHandlerError {
    NoSubcommand,
    ParseError(ParseError),
    SocketError(std::io::Error),
    EncodeFailed(EncodeError),
    DecodeFailed(DecodeError),
}

impl fmt::Display for UdsHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UdsHandlerError::NoSubcommand => write!(f, "subcommand is not present at runtime"),
            UdsHandlerError::ParseError(_) => write!(f, "failed to parse"),
            UdsHandlerError::SocketError(_) => write!(f, "failed to handle socket method"),
            UdsHandlerError::EncodeFailed(_) => write!(f, "failed to encode message"),
            UdsHandlerError::DecodeFailed(_) => write!(f, "failed to decode message"),
        }
    }
}

impl std::error::Error for UdsHandlerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UdsHandlerError::NoSubcommand => None,
            UdsHandlerError::ParseError(ref e) => Some(e),
            UdsHandlerError::SocketError(ref e) => Some(e),
            UdsHandlerError::EncodeFailed(ref e) => Some(e),
            UdsHandlerError::DecodeFailed(ref e) => Some(e),
        }
    }
}

#[derive(Debug)]
pub struct ParseError {
    message: Option<String>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.message {
            Some(msg) => write!(f, "error occurred while parsing: {}", msg),
            None => write!(f, "error occurred while parsing"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl ParseError {
    pub fn new(message: String) -> Self {
        ParseError {
            message: Some(message),
        }
    }
}

#[derive(Debug)]
pub enum UserInputHandlerError {
    NoSubcommand,
    ParseError(ParseError),
    CommandMatchError(clap::Error),
    NotificationError(NotificationError),
    SledError(sled::Error),
}

impl fmt::Display for UserInputHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserInputHandlerError::NoSubcommand => {
                write!(f, "subcommand is not present at runtime")
            }
            UserInputHandlerError::ParseError(e) => write!(f, "failed to parse: {}", e),
            UserInputHandlerError::CommandMatchError(e) => {
                write!(f, "failed to get matches: {}", e.render().ansi())
            }
            UserInputHandlerError::NotificationError(e) => write!(f, "{}", e),
            UserInputHandlerError::SledError(e) => write!(f, "{}", e),
        }
    }
}

// Implement the From trait for UserInputHandlerError to convert from sled::Error
impl From<sled::Error> for UserInputHandlerError {
    fn from(error: sled::Error) -> Self {
        UserInputHandlerError::SledError(error)
    }
}

impl std::error::Error for UserInputHandlerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UserInputHandlerError::NoSubcommand => None,
            UserInputHandlerError::ParseError(ref e) => Some(e),
            UserInputHandlerError::CommandMatchError(ref e) => Some(e),
            UserInputHandlerError::NotificationError(ref e) => Some(e),
            UserInputHandlerError::SledError(ref e) => Some(e),
        }
    }
}
