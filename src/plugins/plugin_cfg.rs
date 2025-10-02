use std::fs;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::common;

pub const MODULE: &str = "cfg";
const CFG_FILE: &str = "cfg.toml";

#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    server: String,
}

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self { msg_tx };

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

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;

        let config_str = fs::read_to_string(CFG_FILE)
            .unwrap_or_else(|_| panic!("Failed to read config file: {CFG_FILE}"));

        let config: Config = toml::from_str(&config_str)
            .unwrap_or_else(|_| panic!("Failed to parse TOML from: {CFG_FILE}"));

        self.info(format!("  Name: {}", config.name)).await;
        self.info(format!("  Server: {}", config.server)).await;
        globals::set_sys_name(&config.name);
        globals::set_server(&config.server);
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Script: {CFG_FILE}")).await;
        self.info(format!("  Name: {}", globals::get_sys_name()))
            .await;
        self.info(format!("  Server: {}", globals::get_server()))
            .await;
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
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
