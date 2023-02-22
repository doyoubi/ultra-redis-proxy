mod command;
pub mod utils;

#[derive(Debug)]
pub struct Config {
    pub proxy_address: String,
    pub backend_address: String,
    pub group_sessions: usize,
}

pub use command::{ReplySender, ReplyReceiver, reply_channel};