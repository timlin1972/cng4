use std::fs;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::common;

pub const MODULE: &str = "cfg";

#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    script_gui: String,
    script_cli: String,
}

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    mode: Mode,
    script: String,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>, mode: Mode, script: &str) -> Result<Self> {
        let myself = Self {
            msg_tx,
            mode,
            script: script.to_string(),
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn cmd(&self, cmd: String) {
        msgs::cmd(&self.msg_tx, MODULE, &cmd).await;
    }

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;

        let config_str = fs::read_to_string(&self.script)
            .unwrap_or_else(|_| panic!("Failed to read config file: {}", &self.script));

        let config: Config = toml::from_str(&config_str)
            .unwrap_or_else(|_| panic!("Failed to parse TOML from: {}", &self.script));

        self.info(format!("  Name: {}", config.name)).await;

        // run script
        let script = match self.mode {
            Mode::Gui => config.script_gui,
            Mode::Cli => config.script_cli,
        };

        self.info(format!("  Running scripts for mode: {:?}", self.mode))
            .await;
        for line in script.lines() {
            // self.info(format!("    {line}")).await;

            let line = line.trim();
            if line.is_empty() {
                continue; // Skip empty lines
            }
            self.cmd(line.to_string()).await;
        }
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Mode: {:?}", self.mode)).await;
        self.info(format!("  Script: {}", self.script)).await;
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
    }
}

#[async_trait]
impl plugins_main::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let (_cmd_parts, action) = match common::get_cmd_action(&cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            _ => {
                self.warn(format!("[{MODULE}] Unsupported action: {action}"))
                    .await
            }
        }
    }
}
