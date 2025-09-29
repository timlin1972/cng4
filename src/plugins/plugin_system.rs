use anyhow::Result;
use async_trait::async_trait;
use sysinfo::Networks;
use tokio::sync::mpsc::Sender;
use tokio::time::Duration;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, DeviceKey, Msg};
use crate::plugins::{plugin_mqtt, plugins_main};
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
            tailscale_ip: get_tailscale_ip(),
            temperature: get_temperature(),
        }
    }

    fn update(&mut self) {
        self.tailscale_ip = get_tailscale_ip();
        self.temperature = get_temperature();
    }
}

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    system_info: SystemInfo,
}

impl Plugin {
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
        self.info(format!(
            "  Tailscale IP: {}",
            get_tailscale_ip_str(&self.system_info.tailscale_ip)
        ))
        .await;
        let uptime_str = time::uptime_str(time::uptime() - self.system_info.ts_start_uptime);
        self.info(format!("  App uptime: {uptime_str}")).await;
        self.info(format!(
            "  Temperature: {}",
            get_temperature_str(self.system_info.temperature)
        ))
        .await;
    }

    async fn handle_cmd_update(&mut self) {
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
            get_tailscale_ip_str(&self.system_info.tailscale_ip)
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

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {}", Action::Update)).await;
    }
}

#[async_trait]
impl plugins_main::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let (_cmd_parts, action) = match common::get_cmd_action(&cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            Action::Update => self.handle_cmd_update().await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref()))
                    .await
            }
        }
    }
}

//
// Helpers
//

const TAILSCALE_INTERFACE: &str = "tailscale";
const TAILSCALE_INTERFACE_MAC: &str = "utun";

fn get_tailscale_ip() -> Option<String> {
    let networks = Networks::new_with_refreshed_list();
    for (interface_name, network) in &networks {
        if interface_name.starts_with(TAILSCALE_INTERFACE) {
            for ipnetwork in network.ip_networks().iter() {
                // if ipv4
                if ipnetwork.addr.is_ipv4() {
                    return Some(ipnetwork.addr.to_string());
                }
            }
        }
        if interface_name.starts_with(TAILSCALE_INTERFACE_MAC) {
            for ipnetwork in network.ip_networks().iter() {
                // if ipv4
                if let std::net::IpAddr::V4(ip) = ipnetwork.addr {
                    // if the first 1 byte is 100, it's a tailscale ip
                    if ip.octets()[0] == 100 {
                        return Some(ipnetwork.addr.to_string());
                    }
                }
            }
        }
    }

    None
}

fn get_tailscale_ip_str(tailscale_ip: &Option<String>) -> String {
    match tailscale_ip {
        Some(ip) => ip.clone(),
        None => "N/A".to_string(),
    }
}

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

fn get_temperature_str(temperature: Option<f32>) -> String {
    match temperature {
        Some(t) => format!("{:.1}°C", t),
        None => "N/A".to_string(),
    }
}

fn get_temperature_mqtt(temperature: Option<f32>) -> String {
    match temperature {
        Some(t) => format!("{:.1}", t),
        None => "0.0".to_string(),
    }
}
