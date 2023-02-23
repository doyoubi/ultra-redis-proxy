mod command;
pub mod utils;

#[derive(Debug)]
pub struct Config {
    pub proxy_address: String,
    pub backend_addresses: Vec<String>,
    pub group_sessions: usize,
}

pub use command::{reply_channel, ReplyReceiver, ReplySender};
