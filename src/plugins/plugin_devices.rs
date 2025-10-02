use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::consts;
use crate::messages::{self as msgs, Action, Data, DeviceKey, InfoKey, Msg};
use crate::plugins::{plugin_infos, plugin_system, plugins_main};
use crate::utils::{self, api, common};

pub const MODULE: &str = "devices";

// DevInfo
#[derive(Debug, Clone)]
pub struct DevInfo {
    pub ts: u64,
    pub name: String,
    pub onboard: bool,
    pub version: Option<String>,
    pub tailscale_ip: Option<String>,
    pub temperature: Option<f32>,
    pub app_uptime: Option<u64>,
}

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    mode: Mode,
    devices: Vec<DevInfo>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>, mode: Mode) -> Result<Self> {
        let myself = Self {
            msg_tx,
            mode,
            devices: Vec::new(),
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn cmd(&self, cmd: String) {
        msgs::cmd(&self.msg_tx, MODULE, &cmd).await;
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!(
            "  {:<12} {:<7} {:<8} {:<15} {:<6} {:<12} {:<16}",
            "Name", "Onboard", "Version", "Tailscale IP", "Temp", "App uptime", "Last update"
        ))
        .await;

        for device in &self.devices {
            self.info(format!(
                "  {:<12} {:<7} {:<8} {:<15} {:<6} {:<12} {:<16}",
                device.name,
                onboard_str(device.onboard),
                version_str(&device.version),
                tailscale_ip_str(&device.tailscale_ip),
                common::temperature_str(device.temperature),
                app_uptime_str(device.app_uptime),
                utils::time::ts_str_local(device.ts)
            ))
            .await;
        }
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {} <device_name> \"<cmd>\"", Action::Cmd))
            .await;
    }

    async fn handle_update_onboard(&mut self, name: &str, value: &str) {
        let onboard = value == "1";
        let ts = utils::time::ts();
        let onboard_str = onboard_str(onboard);

        let changed =
            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                let changed = onboard != device.onboard;

                device.ts = ts;
                device.onboard = onboard;

                changed
            } else {
                let device_add = DevInfo {
                    ts,
                    name: name.to_string(),
                    onboard,
                    version: None,
                    tailscale_ip: None,
                    temperature: None,
                    app_uptime: None,
                };
                self.devices.push(device_add);

                true
            };

        if changed {
            self.info(format!(
                "{name} {onboard_str} at {}",
                utils::time::ts_str_full(ts),
            ))
            .await;

            // someone onboard, publish immediately
            if onboard {
                self.cmd(format!(
                    "{} {} {}",
                    consts::P,
                    plugin_system::MODULE,
                    Action::Update
                ))
                .await;
            }

            // update infos
            if self.mode == Mode::Gui {
                self.cmd(format!(
                    "{} {} {} {} {} {name} {value}",
                    consts::P,
                    plugin_infos::MODULE,
                    Action::Update,
                    InfoKey::Devices,
                    DeviceKey::Onboard,
                ))
                .await;
            }

            // // update nas
            // self.cmd(
            //     MODULE,
            //     format!("p nas {ACTION_DEVICES} onboard {name} {onbard_str}"),
            // )
            // .await;
        }
    }

    async fn handle_update_version(&mut self, name: &str, value: &str) {
        let ts = utils::time::ts();

        if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
            device.ts = ts;
            device.version = Some(value.to_string());

            // update infos
            if self.mode == Mode::Gui {
                self.cmd(format!(
                    "{} {} {} {} {} {name} {value}",
                    consts::P,
                    plugin_infos::MODULE,
                    Action::Update,
                    InfoKey::Devices,
                    DeviceKey::Version,
                ))
                .await;
            }
        }
    }

    async fn handle_update_tailscale_ip(&mut self, name: &str, value: &str) {
        let ts = utils::time::ts();

        if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
            device.ts = ts;
            device.tailscale_ip = Some(value.to_string());

            // update infos
            if self.mode == Mode::Gui {
                self.cmd(format!(
                    "{} {} {} {} {} {name} {value}",
                    consts::P,
                    plugin_infos::MODULE,
                    Action::Update,
                    InfoKey::Devices,
                    DeviceKey::TailscaleIp,
                ))
                .await;
            }

            // // update nas
            // self.cmd(
            //     MODULE,
            //     format!("p nas {ACTION_DEVICES} {ACTION_TAILSCALE_IP} {name} {value}"),
            // )
            // .await;
        }
    }

    async fn handle_update_temperature(&mut self, name: &str, value: &str) {
        let ts = utils::time::ts();
        let temperature: Option<f32> = value.parse::<f32>().ok();

        if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
            device.ts = ts;
            device.temperature = temperature;
            if device.temperature == Some(0.0) {
                device.temperature = None;
            }

            // update infos
            if self.mode == Mode::Gui {
                self.cmd(format!(
                    "{} {} {} {} {} {name} {value}",
                    consts::P,
                    plugin_infos::MODULE,
                    Action::Update,
                    InfoKey::Devices,
                    DeviceKey::Temperature,
                ))
                .await;
            }
        }
    }

    async fn handle_update_app_uptime(&mut self, name: &str, value: &str) {
        let ts = utils::time::ts();
        let app_uptime: Option<u64> = value.parse::<u64>().ok();

        if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
            device.ts = ts;
            device.app_uptime = app_uptime;

            // update infos
            if self.mode == Mode::Gui {
                self.cmd(format!(
                    "{} {} {} {} {} {name} {value}",
                    consts::P,
                    plugin_infos::MODULE,
                    Action::Update,
                    InfoKey::Devices,
                    DeviceKey::AppUptime,
                ))
                .await;
            }
        }
    }

    async fn handle_cmd_update(&mut self, cmd_parts: Vec<String>) {
        if let (Some(device_key), Some(name), Some(value)) =
            (cmd_parts.get(3), cmd_parts.get(4), cmd_parts.get(5))
        {
            match device_key.parse::<DeviceKey>() {
                Ok(_k @ DeviceKey::Onboard) => self.handle_update_onboard(name, value).await,
                Ok(_k @ DeviceKey::Version) => self.handle_update_version(name, value).await,
                Ok(_k @ DeviceKey::TailscaleIp) => {
                    self.handle_update_tailscale_ip(name, value).await
                }
                Ok(_k @ DeviceKey::Temperature) => {
                    self.handle_update_temperature(name, value).await
                }
                Ok(_k @ DeviceKey::AppUptime) => self.handle_update_app_uptime(name, value).await,
                Err(_) => {
                    self.warn(common::MsgTemplate::InvalidParameters.format(
                        "<device_key> (`{device_key}`)",
                        Action::Update.as_ref(),
                        &cmd_parts.join(" "),
                    ))
                    .await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<key> <name> <value>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    pub async fn handle_cmd(&mut self, cmd_parts: Vec<String>) {
        if let (Some(device_name), Some(cmd)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            if let Some(device) = self
                .devices
                .iter()
                .find(|device| device.name == *device_name)
            {
                if let Some(ip) = &device.tailscale_ip {
                    self.info(format!(
                        "Sending command to device `{device_name}` at {ip}: `{cmd}`"
                    ))
                    .await;
                    api::send_cmd(
                        &self.msg_tx,
                        MODULE,
                        ip,
                        &api::CmdRequest { cmd: cmd.clone() },
                    )
                    .await;
                } else {
                    self.warn(format!("Device `{device_name}` has no Tailscale IP"))
                        .await;
                }
            } else {
                self.warn(format!("Device `{device_name}` not found")).await;
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<device_name> \"<cmd>\"",
                Action::Cmd.as_ref(),
                &cmd_parts.join(" "),
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
            Action::Update => self.handle_cmd_update(cmd_parts).await,
            Action::Cmd => self.handle_cmd(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}

pub fn onboard_str(onboard: bool) -> &'static str {
    if onboard { "On" } else { "Off" }
}

pub fn version_str(version: &Option<String>) -> &str {
    version.as_deref().unwrap_or(consts::NA)
}

pub fn tailscale_ip_str(tailscale_ip: &Option<String>) -> &str {
    tailscale_ip.as_deref().unwrap_or(consts::NA)
}

pub fn app_uptime_str(app_uptime: Option<u64>) -> String {
    if let Some(t) = app_uptime {
        utils::time::uptime_str(t)
    } else {
        consts::NA.to_owned()
    }
}
