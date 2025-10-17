use std::net::{IpAddr, ToSocketAddrs};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{Action, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::common;

pub const MODULE: &str = "ping";

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self { msg_tx };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info("  ping <ip_address> - Ping the specified IP address.".to_string())
            .await;
    }

    async fn handle_action_ping(&self, cmd_parts: &[String]) {
        self.info(Action::Ping.to_string()).await;

        let target = match cmd_parts.get(3) {
            Some(t) => t.as_str(),
            None => {
                self.warn(common::MsgTemplate::MissingParameters.format(
                    "<ip_address>",
                    Action::Ping.as_ref(),
                    &cmd_parts.join(" "),
                ))
                .await;
                return;
            }
        };

        self.info(format!("Pinging IP: {target}")).await;

        let ip = match resolve_to_ip(target) {
            Ok(ip) => ip,
            Err(e) => {
                self.warn(format!("Failed to resolve {target}: {e}")).await;

                return;
            }
        };

        let payload = [0; 8];

        let (_packet, duration) = match surge_ping::ping(ip, &payload).await {
            Ok((_packet, duration)) => (_packet, duration),
            Err(e) => {
                self.warn(format!("Failed to ping {ip}: {e}")).await;

                return;
            }
        };

        self.info(format!("Ping took {duration:.3?}")).await;
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
            Action::Ping => self.handle_action_ping(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}

fn resolve_to_ip(input: &str) -> Result<IpAddr> {
    if let Ok(ip) = input.parse::<IpAddr>() {
        return Ok(ip);
    }

    let addrs = (input, 0).to_socket_addrs()?;
    addrs
        .map(|addr| addr.ip())
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No IP address found"))
        .map_err(Into::into)
}
