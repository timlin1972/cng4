use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio::time::Duration;

use crate::consts;
use crate::messages::{self as msgs, Action, DeviceKey, Msg};
use crate::plugins::{
    plugin_mqtt,
    plugins_main::{self, Plugin},
};
use crate::utils::{common, time};

pub const MODULE: &str = "system";
const UPDATE_INTERVAL: u64 = 300;

#[derive(Debug)]
struct SystemInfo {
    ts_start_uptime: u64,
    tailscale_ip: Option<String>,
    temperature: Option<f32>,
}

impl SystemInfo {
    fn new() -> Self {
        Self {
            ts_start_uptime: time::uptime(),
            tailscale_ip: common::get_tailscale_ip(),
            temperature: get_temperature(),
        }
    }

    fn update(&mut self) {
        self.tailscale_ip = common::get_tailscale_ip();
        self.temperature = get_temperature();
    }
}

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    system_info: SystemInfo,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx: msg_tx.clone(),
            system_info: SystemInfo::new(),
        };

        myself.info(consts::NEW.to_string()).await;

        tokio::spawn(async move {
            msgs::info(
                &msg_tx,
                MODULE,
                &format!("  Starting to update every {UPDATE_INTERVAL} secs..."),
            )
            .await;
            loop {
                tokio::time::sleep(Duration::from_secs(UPDATE_INTERVAL)).await;
                msgs::cmd(
                    &msg_tx,
                    MODULE,
                    &format!("{} {MODULE} {}", consts::P, Action::Update),
                )
                .await;
            }
        });

        Ok(myself)
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!(
            "  Tailscale IP: {}",
            common::get_tailscale_ip_str(&self.system_info.tailscale_ip)
        ))
        .await;
        let uptime_str = time::uptime_str(time::uptime() - self.system_info.ts_start_uptime);
        self.info(format!("  App uptime: {uptime_str}")).await;
        self.info(format!(
            "  Temperature: {}",
            common::temperature_str(self.system_info.temperature)
        ))
        .await;
    }

    async fn handle_action_update(&mut self) {
        self.system_info.update();

        // onboard
        self.cmd(format!(
            "{} {} {} false {} '1'",
            consts::P,
            plugin_mqtt::MODULE,
            Action::Publish,
            DeviceKey::Onboard
        ))
        .await;

        // version
        self.cmd(format!(
            "{} {} {} false {} '{}'",
            consts::P,
            plugin_mqtt::MODULE,
            Action::Publish,
            DeviceKey::Version,
            env!("CARGO_PKG_VERSION")
        ))
        .await;

        // tailscale_ip
        self.cmd(format!(
            "{} {} {} false {} '{}'",
            consts::P,
            plugin_mqtt::MODULE,
            Action::Publish,
            DeviceKey::TailscaleIp,
            common::get_tailscale_ip_str(&self.system_info.tailscale_ip)
        ))
        .await;

        // temperature
        self.cmd(format!(
            "{} {} {} false {} '{}'",
            consts::P,
            plugin_mqtt::MODULE,
            Action::Publish,
            DeviceKey::Temperature,
            get_temperature_mqtt(self.system_info.temperature)
        ))
        .await;

        // app uptime
        let uptime = time::uptime() - self.system_info.ts_start_uptime;
        self.cmd(format!(
            "{} {} {} false {} '{}'",
            consts::P,
            plugin_mqtt::MODULE,
            Action::Publish,
            DeviceKey::AppUptime,
            uptime
        ))
        .await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Update)).await;
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
            Action::Update => self.handle_action_update().await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}

//
// Helpers
//

fn get_temperature() -> Option<f32> {
    let components = sysinfo::Components::new_with_refreshed_list();
    for component in &components {
        let component_label = component.label().to_ascii_lowercase();
        if component_label.contains("cpu") || component_label.contains("acpitz") {
            return component.temperature();
        }
    }

    None
}

fn get_temperature_mqtt(temperature: Option<f32>) -> String {
    match temperature {
        Some(t) => format!("{:.1}", t),
        None => "0.0".to_string(),
    }
}
