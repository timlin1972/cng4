use std::fs;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::consts;
use crate::messages::{Action, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::common;

pub const MODULE: &str = "script";

#[derive(Debug, Deserialize)]
struct Config {
    script_gui: String,
    script_cli: String,
}

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    mode: Mode,
    script: String,
}

impl PluginUnit {
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

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;

        let config_str = fs::read_to_string(&self.script)
            .unwrap_or_else(|_| panic!("Failed to read config file: {}", &self.script));

        let config: Config = toml::from_str(&config_str)
            .unwrap_or_else(|_| panic!("Failed to parse TOML from: {}", &self.script));

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

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Mode: {:?}", self.mode)).await;
        self.info(format!("  Script: {}", self.script)).await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
    }
}

#[async_trait]
impl plugins_main::Plugin for PluginUnit {
    fn name(&self) -> &str {
        MODULE
    }

    fn msg_tx(&self) -> &Sender<Msg> {
        &self.msg_tx
    }

    async fn handle_action(&mut self, action: Action, _cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
