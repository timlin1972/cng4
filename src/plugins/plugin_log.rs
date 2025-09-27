use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::{common, time};

pub const MODULE: &str = "log";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self { msg_tx };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_log(&self, ts: u64, plugin: &str, cmd_parts: Vec<String>) {
        if let (Some(level), Some(msg)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            println!("{} {plugin:>10}: [{level}] {msg}", time::ts_str(ts));
        } else {
            println!("[{MODULE}] Incomplete log command: {cmd_parts:?}");
        }
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {} <level> <message>", Action::Log))
            .await;
        self.info("    level: INFO, WARN, ERROR".to_string()).await;
        self.info("    message: the log message".to_string()).await;
    }
}

#[async_trait]
impl plugins_main::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let (cmd_parts, action) = match common::get_cmd_action(&cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                println!("[{MODULE}] {err}");
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            Action::Log => self.handle_cmd_log(msg.ts, &msg.plugin, cmd_parts).await,
            _ => println!("[{MODULE}] Unsupported action: {action}"),
        }
    }
}
