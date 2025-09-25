use std::fmt;

use tokio::sync::mpsc::{self, Sender};

use crate::utils::time;

const MSG_SIZE: usize = 4096;

#[derive(Debug)]
pub struct Msg {
    pub ts: u64,
    pub plugin: String,
}

impl Msg {
    pub fn new(plugin: &str) -> Self {
        Self {
            ts: time::ts(),
            plugin: plugin.to_string(),
        }
    }
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "time: {}, plugin: {}",
            time::ts_str(self.ts),
            self.plugin
        )
    }
}

pub struct Messages {
    pub msg_tx: Sender<Msg>,
}

impl Messages {
    pub async fn new() -> Self {
        let (msg_tx, mut msg_rx) = mpsc::channel::<Msg>(MSG_SIZE);

        tokio::spawn(async move {
            while let Some(msg) = msg_rx.recv().await {
                println!("{msg}");
            }
        });

        Self { msg_tx }
    }
}
