use anyhow::Result;
use async_trait::async_trait;
use colored::*;
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::{plugin_panels, plugins_main};
use crate::utils::{common, time};

pub const MODULE: &str = "log";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    mode: Mode,
    gui_panel: Option<String>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>, mode: Mode) -> Result<Self> {
        let myself = Self {
            msg_tx,
            mode,
            gui_panel: None,
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn cmd(&self, msg: String) {
        msgs::cmd(&self.msg_tx, MODULE, &msg).await;
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_log(&self, ts: u64, plugin: &str, cmd_parts: Vec<String>) {
        if let (Some(level), Some(msg)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            if self.mode == Mode::Gui && self.gui_panel.is_some() {
                self.cmd(format!(
                    "{} {} {} {} '{} {plugin:>10}: [{level}] {msg}'",
                    consts::P,
                    plugin_panels::MODULE,
                    Action::Push,
                    self.gui_panel.as_ref().unwrap(),
                    time::ts_str(ts)
                ))
                .await;
                return;
            }

            let msg = format!("{} {plugin:>10}: [{level}] {msg}", time::ts_str(ts));
            let msg = match level.to_lowercase().as_str() {
                "info" => msg.normal(),
                "warn" => msg.yellow(),
                "error" => msg.red(),
                _ => msg.red().on_yellow(),
            };
            println!("{msg}");
        } else {
            self.warn(format!("Incomplete log command: {cmd_parts:?}"))
                .await;
        }
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Mode: {}", self.mode)).await;
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

    async fn handle_cmd_gui(&mut self, cmd_parts: Vec<String>) {
        if let Some(gui_panel) = cmd_parts.get(3) {
            self.gui_panel = Some(gui_panel.to_string());
        } else {
            self.warn(format!(
                "Missing gui_panel for gui command: `{cmd_parts:?}`"
            ))
            .await;
        }
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
                self.warn(err.clone()).await;
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            Action::Log => self.handle_cmd_log(msg.ts, &msg.plugin, cmd_parts).await,
            Action::Gui => self.handle_cmd_gui(cmd_parts).await,
            _ => self.warn(format!("Unsupported action: {action}")).await,
        }
    }
}
