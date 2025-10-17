use anyhow::Result;
use async_trait::async_trait;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::{cursor::SetCursorStyle, execute},
};
use tokio::sync::{broadcast, mpsc};

use crate::arguments::Mode;
use crate::consts;
use crate::messages::{self as msgs, Action, Data, Key, Msg};
use crate::plugins::{
    plugin_cfg, plugin_cli, plugin_command, plugin_devices, plugin_editor, plugin_gui,
    plugin_infos, plugin_log, plugin_mqtt, plugin_music, plugin_ping, plugin_script, plugin_system,
    plugin_weather, plugin_web, plugin_wol,
};
use crate::utils::{common, panel};

pub const MODULE: &str = "plugins";

#[async_trait]
pub trait Plugin {
    fn name(&self) -> &str;

    fn msg_tx(&self) -> &mpsc::Sender<Msg>;

    fn panel_info(&self) -> &panel::PanelInfo {
        panic!(
            "`panel_info` is not implemented for plugin: `{}`",
            self.name()
        )
    }

    async fn info(&self, msg: String) {
        msgs::info(self.msg_tx(), self.name(), &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(self.msg_tx(), self.name(), &msg).await;
    }

    async fn cmd(&self, msg: String) {
        msgs::cmd(self.msg_tx(), self.name(), &msg).await;
    }

    async fn handle_action_gui(
        &mut self,
        cmd_parts: &[String],
    ) -> anyhow::Result<panel::PanelInfo> {
        if let (Some(panel_type), Some(x), Some(y), Some(w), Some(h)) = (
            cmd_parts.get(3),
            cmd_parts.get(4),
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
        ) {
            msgs::cmd(
                self.msg_tx(),
                self.name(),
                &format!(
                    "{} {} {} {}",
                    consts::P,
                    MODULE,
                    Action::InsertPanel,
                    self.name()
                ),
            )
            .await;

            Ok(panel::PanelInfo {
                panel_type: panel_type.parse::<panel::PanelType>().unwrap(),
                x: x.parse::<u16>().unwrap(),
                y: y.parse::<u16>().unwrap(),
                w: w.parse::<u16>().unwrap(),
                h: h.parse::<u16>().unwrap(),
            })
        } else {
            msgs::warn(
                self.msg_tx(),
                self.name(),
                &common::MsgTemplate::MissingParameters.format(
                    "<panel_type> <x> <y> <width> <height>",
                    Action::Gui.as_ref(),
                    &cmd_parts.join(" "),
                ),
            )
            .await;

            Err(anyhow::anyhow!("Missing parameters"))
        }
    }

    async fn handle_action(&mut self, _action: Action, _cmd_parts: &[String], _msg: &Msg) {
        panic!(
            "`handle_action` is not implemented for plugin: `{}`",
            self.name()
        )
    }

    async fn handle_action_key_position(
        &mut self,
        key: Key,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
    ) -> (u16, u16, u16, u16) {
        let mut new_x = x;
        let mut new_y = y;
        let mut new_w = w;
        let mut new_h = h;
        match key {
            Key::AltUp => {
                new_y = new_y.saturating_sub(1);
            }
            Key::AltDown => {
                // if self.panel_info.y + self.panel_info.h < globals::get_terminal_height() {
                new_y += 1;
                // }
            }
            Key::AltLeft => {
                new_x = new_x.saturating_sub(1);
            }
            Key::AltRight => {
                // if self.panel_info.x + self.panel_info.w < globals::get_terminal_width() {
                new_x += 1;
                // }
            }
            Key::AltW => {
                if new_h > 3 {
                    new_h -= 1;
                }
            }
            Key::AltS => {
                // if self.panel_info.y + self.panel_info.h < globals::get_terminal_height() {
                new_h += 1;
                // }
            }
            Key::AltA => {
                if new_w > 10 {
                    new_w -= 1;
                }
            }
            Key::AltD => {
                // if self.panel_info.x + self.panel_info.w < globals::get_terminal_width() {
                new_w += 1;
                // }
            }
            _ => {}
        }

        msgs::cmd(
            self.msg_tx(),
            self.name(),
            &format!("{} {MODULE} {}", consts::P, Action::Redraw,),
        )
        .await;

        (new_x, new_y, new_w, new_h)
    }

    fn draw(&mut self, _frame: &mut Frame, _active: bool) {
        panic!("`draw` is not implemented for plugin: `{}`", self.name())
    }
}

pub struct Plugins {
    plugins: Vec<Box<dyn Plugin + Send + Sync>>,
    msg_tx: mpsc::Sender<Msg>,
    shutdown_tx: broadcast::Sender<()>,
    mode: Mode,
    script: String,
    terminal: Option<DefaultTerminal>,
    panels: Vec<String>,
    active_panel: usize,
    active_popup: Option<usize>,
}

impl Plugins {
    pub async fn new(
        msg_tx: mpsc::Sender<Msg>,
        shutdown_tx: broadcast::Sender<()>,
        mode: Mode,
        script: &str,
    ) -> Self {
        let mut myself = Self {
            plugins: Vec::new(),
            msg_tx,
            shutdown_tx,
            mode,
            script: script.to_string(),
            terminal: None,
            panels: Vec::new(),
            active_panel: 0,
            active_popup: None,
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        myself
    }

    async fn init(&mut self) {
        self.info(consts::INIT.to_string()).await;

        if self.mode == Mode::Cli {
            return;
        }

        self.terminal = Some(ratatui::init());

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;

            let mut stdout = std::io::stdout();
            execute!(stdout, SetCursorStyle::DefaultUserShape).unwrap();

            ratatui::restore();
        });
    }

    pub async fn insert(&mut self, plugin: &str) -> Result<()> {
        // return if plugin is already inserted
        if self.get_plugin_mut(plugin).is_some() {
            return Err(anyhow::anyhow!("Plugin `{plugin}` is already inserted."));
        }

        let plugin = match plugin {
            plugin_log::MODULE => {
                Box::new(plugin_log::PluginUnit::new(self.msg_tx.clone(), self.mode.clone()).await?)
                    as Box<dyn Plugin + Send + Sync>
            }
            plugin_cfg::MODULE => Box::new(plugin_cfg::PluginUnit::new(self.msg_tx.clone()).await?)
                as Box<dyn Plugin + Send + Sync>,
            plugin_system::MODULE => {
                Box::new(plugin_system::PluginUnit::new(self.msg_tx.clone()).await?)
                    as Box<dyn Plugin + Send + Sync>
            }
            plugin_cli::MODULE => Box::new(plugin_cli::PluginUnit::new(self.msg_tx.clone()).await?)
                as Box<dyn Plugin + Send + Sync>,
            plugin_web::MODULE => Box::new(plugin_web::PluginUnit::new(self.msg_tx.clone()).await?)
                as Box<dyn Plugin + Send + Sync>,
            plugin_music::MODULE => {
                Box::new(plugin_music::PluginUnit::new(self.msg_tx.clone()).await?)
                    as Box<dyn Plugin + Send + Sync>
            }
            plugin_gui::MODULE => Box::new(
                plugin_gui::PluginUnit::new(self.msg_tx.clone(), self.shutdown_tx.clone()).await?,
            ) as Box<dyn Plugin + Send + Sync>,
            plugin_mqtt::MODULE => Box::new(
                plugin_mqtt::PluginUnit::new(
                    self.msg_tx.clone(),
                    self.shutdown_tx.clone(),
                    self.mode.clone(),
                )
                .await?,
            ) as Box<dyn Plugin + Send + Sync>,
            plugin_devices::MODULE => Box::new(
                plugin_devices::PluginUnit::new(self.msg_tx.clone(), self.mode.clone()).await?,
            ) as Box<dyn Plugin + Send + Sync>,
            plugin_infos::MODULE => {
                Box::new(plugin_infos::PluginUnit::new(self.msg_tx.clone()).await?)
                    as Box<dyn Plugin + Send + Sync>
            }
            plugin_script::MODULE => Box::new(
                plugin_script::PluginUnit::new(
                    self.msg_tx.clone(),
                    self.mode.clone(),
                    &self.script,
                )
                .await?,
            ) as Box<dyn Plugin + Send + Sync>,
            plugin_weather::MODULE => Box::new(
                plugin_weather::PluginUnit::new(self.msg_tx.clone(), self.mode.clone()).await?,
            ) as Box<dyn Plugin + Send + Sync>,
            plugin_editor::MODULE => {
                Box::new(plugin_editor::PluginUnit::new(self.msg_tx.clone()).await?)
                    as Box<dyn Plugin + Send + Sync>
            }
            plugin_command::MODULE => {
                Box::new(plugin_command::PluginUnit::new(self.msg_tx.clone()).await?)
                    as Box<dyn Plugin + Send + Sync>
            }
            plugin_wol::MODULE => Box::new(plugin_wol::PluginUnit::new(self.msg_tx.clone()).await?)
                as Box<dyn Plugin + Send + Sync>,
            plugin_ping::MODULE => {
                Box::new(plugin_ping::PluginUnit::new(self.msg_tx.clone()).await?)
                    as Box<dyn Plugin + Send + Sync>
            }
            _ => return Err(anyhow::anyhow!("Unknown plugin name: `{plugin}`")),
        };

        self.plugins.push(plugin);
        Ok(())
    }

    async fn handle_action_insert(&mut self, cmd_parts: &[String]) {
        self.info(Action::Insert.to_string()).await;

        if let Some(plugin) = cmd_parts.get(3) {
            self.info(format!("  - `{plugin}`")).await;
            if let Err(e) = self.insert(plugin).await {
                self.warn(e.to_string()).await;
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<plugin>",
                Action::Insert.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;

        self.info("  - Plugins:".to_string()).await;
        for plugin in &self.plugins {
            self.info(format!("    - {}", plugin.name())).await;
        }

        if self.panels.is_empty() {
            self.info("  - Panels:".to_string()).await;
            self.info("    - <none>".to_string()).await;
        } else {
            self.info("  - Panels:".to_string()).await;
            for panel in &self.panels {
                self.info(format!("    - {}", panel)).await;
            }
        }
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {} <plugin>", Action::Insert)).await;
    }

    fn draw(&mut self, frame: &mut Frame) {
        let panels = self.panels.clone();
        let active_panel = self.active_panel;
        let active_popup = self.active_popup;

        for (idx, panel) in panels.iter().enumerate() {
            if idx == active_panel {
                continue;
            }
            #[allow(clippy::collapsible_if)]
            if let Some(plugin) = self.get_plugin_mut(panel) {
                if plugin.panel_info().panel_type == panel::PanelType::Normal {
                    plugin.draw(frame, active_popup.is_none() && idx == active_panel);
                }
            }
        }

        for (idx, panel) in panels.iter().enumerate() {
            if idx != active_panel {
                continue;
            }
            #[allow(clippy::collapsible_if)]
            if let Some(plugin) = self.get_plugin_mut(panel) {
                if plugin.panel_info().panel_type == panel::PanelType::Normal {
                    plugin.draw(frame, active_popup.is_none() && idx == active_panel);
                }
            }
        }

        // for popup panels
        if let Some(active_popup) = active_popup {
            for (idx, panel) in panels.iter().enumerate() {
                #[allow(clippy::collapsible_if)]
                if let Some(plugin) = self.get_plugin_mut(panel) {
                    if plugin.panel_info().panel_type == panel::PanelType::Popup
                        && idx == active_popup
                    {
                        plugin.draw(frame, true);
                        break;
                    }
                }
            }
        }
    }

    fn redraw(&mut self) {
        let mut terminal = self.terminal.take().unwrap();
        let _ = terminal.draw(|frame| self.draw(frame));
        self.terminal = Some(terminal);
    }

    async fn handle_action_redraw(&mut self) {
        self.redraw();
    }

    async fn handle_action_insert_panel(&mut self, cmd_parts: &[String]) {
        self.info(Action::InsertPanel.to_string()).await;

        if let Some(panel) = cmd_parts.get(3) {
            if !self.panels.contains(panel) {
                self.info(format!("  - `{panel}`")).await;
                self.panels.push(panel.to_string());

                self.redraw();
            } else {
                self.warn(format!("Panel `{panel}` is already inserted."))
                    .await;
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<panel>",
                Action::InsertPanel.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_action_key_tab(&mut self) {
        if self.active_popup.is_none() {
            self.active_panel = (self.active_panel + 1) % self.panels.len();
            loop {
                let panel = &self.panels[self.active_panel];
                if let Some(plugin) = self.get_plugin(panel) {
                    if plugin.panel_info().panel_type == panel::PanelType::Popup {
                        self.active_panel = (self.active_panel + 1) % self.panels.len();
                        continue;
                    }
                    break;
                }
            }
        }

        self.redraw();
    }

    async fn handle_action_key_key(&self, key: Key) {
        match self.active_popup {
            None => {
                let panel = &self.panels[self.active_panel];
                self.cmd(format!("{} {panel} {} {key}", consts::P, Action::Key,))
                    .await;
            }
            Some(active_popup) => {
                let panel = &self.panels[active_popup];

                self.cmd(format!("{} {panel} {} {key}", consts::P, Action::Key,))
                    .await;
            }
        }
    }

    async fn handle_action_key(&mut self, cmd_parts: &[String]) {
        if let Some(key) = cmd_parts.get(3) {
            match key.parse::<Key>() {
                Ok(k @ Key::Up)
                | Ok(k @ Key::Down)
                | Ok(k @ Key::Left)
                | Ok(k @ Key::Right)
                | Ok(k @ Key::Home)
                | Ok(k @ Key::End)
                | Ok(k @ Key::AltC)
                | Ok(k @ Key::AltUp)
                | Ok(k @ Key::AltDown)
                | Ok(k @ Key::AltLeft)
                | Ok(k @ Key::AltRight)
                | Ok(k @ Key::AltW)
                | Ok(k @ Key::AltS)
                | Ok(k @ Key::AltA)
                | Ok(k @ Key::AltD)
                | Ok(k @ Key::ControlX) => self.handle_action_key_key(k).await,
                Ok(Key::Tab) => self.handle_action_key_tab().await,
                _ => (),
            }
        }
    }

    async fn handle_action_popup(&mut self, cmd_parts: &[String]) {
        self.info(Action::Popup.to_string()).await;

        if let Some(panel) = cmd_parts.get(3) {
            for (idx, p) in self.panels.iter().enumerate() {
                if p == panel {
                    self.active_popup = Some(idx);
                    break;
                }
            }
        } else {
            self.active_popup = None;
        }

        self.redraw();
    }

    async fn my_handle_action(&mut self, action: Action, cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Insert => self.handle_action_insert(cmd_parts).await,
            Action::InsertPanel => self.handle_action_insert_panel(cmd_parts).await,
            Action::Redraw => self.handle_action_redraw().await,
            Action::Key => self.handle_action_key(cmd_parts).await,
            Action::Popup => self.handle_action_popup(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
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

    pub async fn handle_cmd(&mut self, msg: &Msg) {
        fn get_action(cmd_parts: &[String]) -> Result<Action, String> {
            let action = match cmd_parts.get(2) {
                Some(action) => action,
                None => {
                    return Err(common::MsgTemplate::MissingParameters.format(
                        "<action>",
                        &format!("{} <plugin_name> <action> ...", consts::P),
                        &cmd_parts.join(" "),
                    ));
                }
            };

            let action: Action = match action.parse() {
                Ok(action) => action,
                Err(_) => {
                    return Err(common::MsgTemplate::InvalidParameters.format(
                        &format!("<action> (`{action}`)"),
                        &format!("{} <plugin_name> <action> ...", consts::P),
                        &cmd_parts.join(" "),
                    ));
                }
            };

            Ok(action)
        }

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
                self.warn(common::MsgTemplate::MissingParameters.format(
                    "<plugin_name>",
                    &format!("{} <plugin_name> <action> ...", consts::P),
                    &cmd_parts.join(" "),
                ))
                .await;
                return;
            }
        };

        if plugin_name == MODULE {
            let action = match get_action(&cmd_parts) {
                Ok(action) => action,
                Err(err) => {
                    self.warn(err).await;
                    return;
                }
            };

            self.my_handle_action(action, &cmd_parts, msg).await;
        } else if let Some(plugin) = self.get_plugin_mut(plugin_name) {
            let action = match get_action(&cmd_parts) {
                Ok(action) => action,
                Err(err) => {
                    self.warn(err).await;
                    return;
                }
            };
            plugin.handle_action(action, &cmd_parts, msg).await;
        } else {
            self.warn(common::MsgTemplate::InvalidParameters.format(
                &format!("<plugin_name> (`{plugin_name}`)"),
                &format!("{} <plugin_name> <action> ...", consts::P),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    pub fn get_plugin_mut(&mut self, name: &str) -> Option<&mut Box<dyn Plugin + Send + Sync>> {
        self.plugins.iter_mut().find(|p| p.name() == name)
    }

    #[allow(clippy::borrowed_box)]
    pub fn get_plugin(&self, name: &str) -> Option<&Box<dyn Plugin + Send + Sync>> {
        self.plugins.iter().find(|p| p.name() == name)
    }
}
