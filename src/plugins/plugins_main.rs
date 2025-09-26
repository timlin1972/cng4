use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::{plugin_cfg, plugin_log, plugin_system};
use crate::utils::common;

pub const MODULE: &str = "plugins";

#[async_trait]
pub trait Plugin {
    fn name(&self) -> &str;
    async fn handle_cmd(&mut self, _msg: &Msg) {
        panic!(
            "`handle_cmd` is not implemented for plugin: `{}`",
            self.name()
        )
    }
}

pub struct Plugins {
    plugins: Vec<Box<dyn Plugin + Send + Sync>>,
    msg_tx: tokio::sync::mpsc::Sender<Msg>,
    mode: Mode,
    script: String,
}

impl Plugins {
    pub async fn new(msg_tx: Sender<Msg>, mode: Mode, script: &str) -> Self {
        Self {
            plugins: Vec::new(),
            msg_tx,
            mode,
            script: script.to_string(),
        }
    }

    pub async fn insert(&mut self, plugin: &str) {
        self.info(format!("Inserting plugin: `{plugin}`")).await;

        let plugin = match plugin {
            plugin_log::MODULE => Box::new(plugin_log::Plugin::new(self.msg_tx.clone()).await)
                as Box<dyn Plugin + Send + Sync>,
            plugin_cfg::MODULE => Box::new(
                plugin_cfg::Plugin::new(self.msg_tx.clone(), self.mode.clone(), &self.script).await,
            ) as Box<dyn Plugin + Send + Sync>,
            plugin_system::MODULE => {
                Box::new(plugin_system::Plugin::new(self.msg_tx.clone()).await)
                    as Box<dyn Plugin + Send + Sync>
            }
            _ => panic!("Unknown plugin: `{plugin}`"),
        };

        self.plugins.push(plugin);
    }

    async fn handle_cmd_insert(&mut self, cmd_parts: Vec<String>) {
        if let Some(plugin) = cmd_parts.get(3) {
            self.insert(plugin).await;
        } else {
            self.warn(format!(
                "Missing plugin name for insert command: {cmd_parts:?}"
            ))
            .await;
        }
    }

    async fn handle_cmd_show(&self) {
        let plugin_names: Vec<String> = self.plugins.iter().map(|p| p.name().to_string()).collect();
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Plugins: {plugin_names:?}")).await;
    }

    async fn my_handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let (cmd_parts, action) = match common::get_cmd_action(&cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Insert => self.handle_cmd_insert(cmd_parts).await,
            Action::Show => self.handle_cmd_show().await,
            _ => self.warn(format!("Unsupported action: {action}")).await,
        }
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    pub async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let cmd_parts = match shell_words::split(&cmd.cmd) {
            Ok(parts) => parts,
            Err(_) => {
                self.warn(format!("Failed to parse cmd `{}`.", cmd.cmd))
                    .await;
                return;
            }
        };

        let plugin_name = match cmd_parts.get(1) {
            Some(name) => name,
            None => {
                self.warn(format!("Missing plugin name for cmd `{}`.", cmd.cmd))
                    .await;
                return;
            }
        };

        if plugin_name == MODULE {
            self.my_handle_cmd(msg).await;
        } else if let Some(plugin) = self.get_plugin_mut(plugin_name) {
            plugin.handle_cmd(msg).await;
        } else {
            self.warn(format!(
                "Unknown plugin name (`{plugin_name}`) for cmd `{}`.",
                cmd.cmd
            ))
            .await;
        }
    }

    fn get_plugin_mut(&mut self, name: &str) -> Option<&mut Box<dyn Plugin + Send + Sync>> {
        self.plugins.iter_mut().find(|p| p.name() == name)
    }
}
