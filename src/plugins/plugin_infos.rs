use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, DeviceKey, InfoKey, Msg};
use crate::plugins::{plugin_devices, plugin_panels, plugins_main};
use crate::utils::{self, common};

pub const MODULE: &str = "infos";
const PAGES: usize = 1;

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    gui_panel: Option<String>,
    page_idx: usize,
    sub_title: Vec<String>,
    // page 0
    devices: Vec<plugin_devices::DevInfo>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx,
            gui_panel: None,
            page_idx: 0,
            sub_title: vec!["Devices".to_string()],
            devices: Vec::new(),
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn output_update(&self, msg: &str) {
        msgs::cmd(
            &self.msg_tx,
            MODULE,
            &format!(
                "{} {} {} {} '{}'",
                consts::P,
                plugin_panels::MODULE,
                Action::OutputUpdate,
                self.gui_panel.as_deref().unwrap(),
                msg
            ),
        )
        .await;
    }

    async fn update_devices(&self) -> String {
        let mut output = format!(
            "{:<12} {:<7} {:<10} {:16} {:<7} {:13} {:<16}",
            "Name", "Onboard", "Version", "Tailscale IP", "Temp", "App Uptime", "Last Update"
        );

        for device in &self.devices {
            output += &format!(
                "\n{:<12} {:<7} {:<10} {:16} {:<7} {:13} {:<16}",
                device.name,
                plugin_devices::onboard_str(device.onboard),
                plugin_devices::version_str(&device.version),
                plugin_devices::tailscale_ip_str(&device.tailscale_ip),
                plugin_devices::temperature_str(device.temperature),
                plugin_devices::app_uptime_str(device.app_uptime),
                utils::time::ts_str_no_tz_no_sec(device.ts),
            );
        }

        output
    }

    async fn update(&mut self) {
        // update sub_title
        let sub_title = format!(
            " - {}/{PAGES} - {}",
            self.page_idx + 1,
            self.sub_title[self.page_idx]
        );
        self.cmd(format!(
            "{} {} {} {} '{sub_title}'",
            consts::P,
            plugin_panels::MODULE,
            Action::SubTitle,
            self.gui_panel.as_deref().unwrap()
        ))
        .await;

        let output = match self.page_idx {
            0 => self.update_devices().await,
            _ => "No data".to_string(),
        };

        self.output_update(&output).await;
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

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Gui panel: {:?}", self.gui_panel))
            .await;
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
    }

    async fn handle_cmd_gui(&mut self, cmd_parts: Vec<String>) {
        if let Some(gui_panel) = cmd_parts.get(3) {
            self.gui_panel = Some(gui_panel.to_string());
        } else {
            self.warn(format!(
                "Missing {} for gui command: `{cmd_parts:?}`",
                Action::Gui
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_onboard(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(onboard)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            let onboard = onboard == "1";
            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.onboard = onboard;
            } else {
                let device_add = plugin_devices::DevInfo {
                    ts,
                    name: name.to_string(),
                    onboard,
                    version: None,
                    tailscale_ip: None,
                    temperature: None,
                    app_uptime: None,
                };
                self.devices.push(device_add.clone());
            }
        } else {
            self.warn(format!(
                "Missing name or value for {} command: `{cmd_parts:?}`",
                Action::Update,
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_version(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(version)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.version = Some(version.to_string());
            }
        } else {
            self.warn(format!(
                "Missing name or version for {} command: `{cmd_parts:?}`",
                Action::Update,
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_tailscale_ip(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(tailscale_ip)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.tailscale_ip = Some(tailscale_ip.to_string());
            }
        } else {
            self.warn(format!(
                "Missing name or tailscale_ip for {} command: `{cmd_parts:?}`",
                Action::Update,
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_temperature(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(temperature)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.temperature = Some(temperature.parse::<f32>().unwrap());
                if device.temperature == Some(0.0) {
                    device.temperature = None;
                }
            }
        } else {
            self.warn(format!(
                "Missing name or temperature for {} command: `{cmd_parts:?}`",
                Action::Update,
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_app_uptime(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(app_uptime)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.app_uptime = Some(app_uptime.parse::<u64>().unwrap());
            }
        } else {
            self.warn(format!(
                "Missing name or app_uptime for {} command: `{cmd_parts:?}`",
                Action::Update,
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices(&mut self, cmd_parts: Vec<String>) {
        if let Some(device_key) = cmd_parts.get(4) {
            match device_key.parse::<DeviceKey>() {
                Ok(DeviceKey::Onboard) => self.handle_cmd_update_devices_onboard(cmd_parts).await,
                Ok(DeviceKey::Version) => self.handle_cmd_update_devices_version(cmd_parts).await,
                Ok(DeviceKey::TailscaleIp) => {
                    self.handle_cmd_update_devices_tailscale_ip(cmd_parts).await
                }
                Ok(DeviceKey::Temperature) => {
                    self.handle_cmd_update_devices_temperature(cmd_parts).await
                }
                Ok(DeviceKey::AppUptime) => {
                    self.handle_cmd_update_devices_app_uptime(cmd_parts).await
                }
                _ => {
                    self.warn(format!(
                        "Unknown device_key for {} command: `{cmd_parts:?}`",
                        Action::Update
                    ))
                    .await;
                }
            }
        } else {
            self.warn(format!(
                "Missing device_key for {} command: `{cmd_parts:?}`",
                Action::Update,
            ))
            .await;
        }
    }

    async fn handle_cmd_update(&mut self, cmd_parts: Vec<String>) {
        if let Some(info_key) = cmd_parts.get(3) {
            match info_key.parse::<InfoKey>() {
                Ok(InfoKey::Devices) => self.handle_cmd_update_devices(cmd_parts).await,
                _ => {
                    self.warn(format!(
                        "Unknown info_key for {} command: `{cmd_parts:?}`",
                        Action::Update
                    ))
                    .await;
                    return;
                }
            }
            // update gui
            self.update().await;
        } else {
            self.warn(format!(
                "Missing info_key for {} command: `{cmd_parts:?}`",
                Action::Update
            ))
            .await;
        }
    }

    async fn handle_cmd_key(&mut self, cmd_parts: Vec<String>) {
        if let Some(key) = cmd_parts.get(3) {
            match key.parse::<msgs::Key>() {
                Ok(msgs::Key::Left) => {
                    if self.page_idx > 0 {
                        self.page_idx -= 1;
                    } else {
                        self.page_idx = PAGES - 1;
                    }
                }
                Ok(msgs::Key::Right) => {
                    if self.page_idx + 1 < PAGES {
                        self.page_idx += 1;
                    } else {
                        self.page_idx = 0;
                    }
                }
                _ => (), // ignore other keys
            }
            self.update().await;
        } else {
            self.warn(format!(
                "Missing key for {} command: `{cmd_parts:?}`",
                Action::Key
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
            Action::Gui => self.handle_cmd_gui(cmd_parts).await,
            Action::Update => self.handle_cmd_update(cmd_parts).await,
            Action::Key => self.handle_cmd_key(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref()))
                    .await
            }
        }
    }
}
