use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{Action, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::common;

pub const MODULE: &str = "wol";
const ADD_PARAMS: &str = "<name> <mac_address>";

#[derive(Debug)]
struct Wol {
    name: String,
    mac: [u8; 6],
}

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    wol: Vec<Wol>,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx,
            wol: Vec::new(),
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
        if self.wol.is_empty() {
            self.info("  No devices configured.".to_string()).await;
        } else {
            for device in &self.wol {
                self.info(format!(
                    "  {}: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    device.name,
                    device.mac[0],
                    device.mac[1],
                    device.mac[2],
                    device.mac[3],
                    device.mac[4],
                    device.mac[5],
                ))
                .await;
            }
        }
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {} {ADD_PARAMS}   Add a new device", Action::Add))
            .await;
        self.info(format!(
            "  {} <name>       Wake a device by name",
            Action::Wake
        ))
        .await;
    }

    async fn handle_action_add(&mut self, cmd_parts: &[String]) {
        self.info(Action::Add.to_string()).await;

        if let (Some(name), Some(mac_str)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            self.info(format!("  Adding device `{name}` with MAC `{mac_str}`"))
                .await;
            match common::parse_mac(mac_str) {
                Ok(mac) => {
                    if self.wol.iter().any(|d| d.name == *name) {
                        self.warn(format!("Device `{name}` already exists.")).await;
                    } else {
                        self.wol.push(Wol {
                            name: name.to_string(),
                            mac,
                        });
                        self.info(format!("Device `{name}` added.")).await;
                    }
                }
                Err(e) => {
                    self.warn(format!("Invalid MAC address `{mac_str}`: {e}"))
                        .await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                ADD_PARAMS,
                Action::Add.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_action_wake(&self, cmd_parts: &[String]) {
        self.info(Action::Wake.to_string()).await;

        if let Some(name) = cmd_parts.get(3) {
            self.info(format!("  Waking device `{name}`")).await;
            match self.wol.iter().find(|d| d.name == *name) {
                Some(device) => match wol::send_wol(wol::MacAddr(device.mac), None, None) {
                    Ok(_) => {
                        self.info(format!("Device `{name}` woken up successfully."))
                            .await;
                    }
                    Err(e) => {
                        self.warn(format!("Failed to wake device `{name}`: {e}"))
                            .await;
                    }
                },
                None => {
                    self.warn(format!("Device `{name}` not found.")).await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<name>",
                Action::Wake.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
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

    async fn handle_action(&mut self, action: Action, cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Add => self.handle_action_add(cmd_parts).await,
            Action::Wake => self.handle_action_wake(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
