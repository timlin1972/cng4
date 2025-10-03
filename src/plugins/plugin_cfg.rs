use std::fs;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::globals;
use crate::messages::{Action, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::common;

pub const MODULE: &str = "cfg";
const CFG_FILE: &str = "cfg.toml";

#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    server: String,
}

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self { msg_tx };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
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

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Script: {CFG_FILE}")).await;
        self.info(format!("  Name: {}", globals::get_sys_name()))
            .await;
        self.info(format!("  Server: {}", globals::get_server()))
            .await;
        self.info(format!(
            "  Server IP: {}",
            globals::get_server_ip().unwrap_or_default()
        ))
        .await;
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
