use anyhow::Result;
use async_trait::async_trait;
use colored::*;
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::{plugin_panels, plugins_main};
use crate::utils::{api, common, time};

pub const MODULE: &str = "log";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    mode: Mode,
    gui_panel: Option<String>,
    dest: Option<String>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>, mode: Mode) -> Result<Self> {
        let myself = Self {
            msg_tx,
            mode,
            gui_panel: None,
            dest: None,
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
            if let Some(dest) = &self.dest {
                api::post_log(
                    dest,
                    &api::LogRequest {
                        data: api::LogData {
                            name: globals::get_sys_name(),
                            ts,
                            level: level.to_string(),
                            plugin: plugin.to_string(),
                            msg: msg.to_string(),
                        },
                    },
                )
                .await;
            }

            if self.mode == Mode::Gui && self.gui_panel.is_some() {
                self.cmd(format!(
                    "{} {} {} {} '{} {plugin:>10}: [{}] {msg}'",
                    consts::P,
                    plugin_panels::MODULE,
                    Action::OutputPush,
                    self.gui_panel.as_ref().unwrap(),
                    time::ts_str(ts),
                    common::level_str(level)
                ))
                .await;
                return;
            }

            let msg = format!(
                "{} {plugin:>10}: [{}] {msg}",
                time::ts_str(ts),
                common::level_str(level)
            );
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
        self.info(format!("  Gui panel: {:?}", self.gui_panel))
            .await;
        self.info(format!("  Dest: {:?}", self.dest)).await;
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {} <level> <message>", Action::Log))
            .await;
        self.info("    level: INFO, WARN, ERROR".to_string()).await;
        self.info("    message: the log message".to_string()).await;
        self.info(format!("  {} <gui_panel>", Action::Gui)).await;
        self.info("    gui_panel: the GUI panel to send log messages to".to_string())
            .await;
        self.info(format!("  {} <dest>", Action::Dest)).await;
        self.info("    dest: the destination IP to send log messages to".to_string())
            .await;
    }

    async fn handle_cmd_gui(&mut self, cmd_parts: Vec<String>) {
        if let Some(gui_panel) = cmd_parts.get(3) {
            self.gui_panel = Some(gui_panel.to_string());
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<gui_panel>",
                Action::Gui.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_cmd_dest(&mut self, cmd_parts: Vec<String>) {
        if let Some(dest) = cmd_parts.get(3) {
            self.dest = Some(dest.to_string());
        } else {
            self.dest = None;
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
            Action::Dest => self.handle_cmd_dest(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
