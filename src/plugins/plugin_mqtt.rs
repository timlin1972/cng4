use anyhow::Result;
use async_trait::async_trait;
use log::Level::{Error, Info, Warn};
use rumqttc::{AsyncClient, Event, Incoming, LastWill, MqttOptions, Publish, QoS};
use tokio::sync::{broadcast, mpsc::Sender};

use crate::arguments::Mode;
use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, Data, DeviceKey, Msg};
use crate::plugins::{plugin_devices, plugin_panels, plugins_main};
use crate::utils::{self, common};

pub const MODULE: &str = "mqtt";
const BROKER: &str = "broker.emqx.io";
const BROKER_PORT: u16 = 1883;
const MQTT_KEEP_ALIVE: u64 = 300;
const RESTART_DELAY: u64 = 60;
const TOPIC_PREFIX: &str = "tln";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    shutdown_tx: broadcast::Sender<()>,
    mode: Mode,
    gui_panel: Option<String>,
    client: Option<AsyncClient>,
}

impl Plugin {
    pub async fn new(
        msg_tx: Sender<Msg>,
        shutdown_tx: broadcast::Sender<()>,
        mode: Mode,
    ) -> Result<Self> {
        let myself = Self {
            msg_tx,
            shutdown_tx,
            mode,
            gui_panel: None,
            client: None,
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn restart(&mut self) {
        let sys_name = globals::get_sys_name();

        // 1. Initialization
        self.info("1/5: Initialization".to_string()).await;

        let mut mqttoptions = MqttOptions::new(sys_name.clone(), BROKER, BROKER_PORT);
        let will = LastWill::new(
            format!("{TOPIC_PREFIX}/{sys_name}/{}", DeviceKey::Onboard),
            "0",
            QoS::AtLeastOnce,
            true,
        );

        mqttoptions
            .set_keep_alive(std::time::Duration::from_secs(MQTT_KEEP_ALIVE))
            .set_last_will(will);

        // 2. Establish connection
        self.info("2/5: Establish connection".to_string()).await;

        let (client, mut connection) = AsyncClient::new(mqttoptions, 10);

        // 3. Subscribe
        self.info("3/5: Subscribe".to_string()).await;

        client
            .subscribe(format!("{TOPIC_PREFIX}/#"), QoS::AtMostOnce)
            .await
            .expect("Failed to subscribe");

        // 4. Publish
        self.info("4/5: Publish".to_string()).await;

        client
            .publish(
                format!("{TOPIC_PREFIX}/{sys_name}/{}", DeviceKey::Onboard),
                QoS::AtLeastOnce,
                true,
                "1",
            )
            .await
            .expect("Failed to publish");

        // 5. Receive
        let msg_tx_clone = self.msg_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let gui_panel_clone = self.gui_panel.clone();
        let mode_clone = self.mode.clone();
        let client_clone = client.clone();

        tokio::spawn(async move {
            msgs::info(&msg_tx_clone, MODULE, "5/5: Receive").await;

            let mut shoutdown_flag = false;
            loop {
                tokio::select! {
                    event = connection.poll() => {
                        if process_event(&msg_tx_clone, &mode_clone, &gui_panel_clone, event).await {
                            break;
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        shoutdown_flag = true;
                        break;
                    }
                }
            }

            msgs::warn(&msg_tx_clone, MODULE, "MQTT client disconnecting...").await;

            client_clone
                .disconnect()
                .await
                .expect("Failed to disconnect");

            msgs::cmd(
                &msg_tx_clone,
                MODULE,
                &format!("{} {MODULE} {}", consts::P, Action::Disconnected),
            )
            .await;

            if !shoutdown_flag {
                // restart in RESTART_DELAY seconds
                tokio::time::sleep(tokio::time::Duration::from_secs(RESTART_DELAY)).await;

                msgs::cmd(
                    &msg_tx_clone,
                    MODULE,
                    &format!("{} {MODULE} {}", consts::P, Action::Restart),
                )
                .await;
            }
        });

        self.client = Some(client);
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Mode: {}", self.mode)).await;
        self.info(format!("  Gui panel: {:?}", self.gui_panel))
            .await;
        self.info(format!(
            "  MQTT Client connected: {}",
            self.client.is_some()
        ))
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
                "Missing gui_panel for {} command: `{cmd_parts:?}`",
                Action::Gui
            ))
            .await;
        }
    }

    async fn handle_cmd_restart(&mut self) {
        self.info(Action::Restart.to_string()).await;
        self.restart().await;
    }

    async fn handle_cmd_disconnected(&mut self) {
        self.info(Action::Disconnected.to_string()).await;
        self.client = None;
    }

    async fn publish(&mut self, topic: &str, retain: bool, payload: &str) {
        if let Some(client) = &self.client {
            let re = regex::Regex::new(&format!(r"^{TOPIC_PREFIX}/([^/]+)/([^/]+)$"))
                .expect("Failed to regex");
            if let Some(captures) = re.captures(topic) {
                let name = &captures[1];
                let key = &captures[2];

                if let Err(e) = client
                    .publish(topic, QoS::AtLeastOnce, retain, payload)
                    .await
                {
                    self.warn(format!(
                        "Failed to publish topic (`{topic}`) payload (`{payload}`). Err: {e:?}"
                    ))
                    .await;
                } else {
                    output_push(
                        &self.msg_tx,
                        &self.mode,
                        &self.gui_panel,
                        Info,
                        format!("📤 pub:: {key} {name} {payload}"),
                    )
                    .await;
                }
            }
        }
    }

    async fn handle_cmd_publish(&mut self, cmd_parts: Vec<String>) {
        if let (Some(retain), Some(key), Some(payload)) =
            (cmd_parts.get(3), cmd_parts.get(4), cmd_parts.get(5))
        {
            let retain = retain == "true";
            self.publish(
                &format!("{TOPIC_PREFIX}/{}/{}", globals::get_sys_name(), key),
                retain,
                payload,
            )
            .await;
        } else {
            self.warn(format!(
                "Missing parameters for {} command: `{cmd_parts:?}`",
                Action::Publish
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
            Action::Restart => self.handle_cmd_restart().await,
            Action::Disconnected => self.handle_cmd_disconnected().await,
            Action::Publish => self.handle_cmd_publish(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref()))
                    .await
            }
        }
    }
}

async fn process_event(
    msg_tx: &Sender<Msg>,
    mode: &Mode,
    gui_panel: &Option<String>,
    event: Result<Event, rumqttc::ConnectionError>,
) -> bool {
    match event {
        Ok(Event::Incoming(Incoming::Publish(publish))) => {
            process_event_publish(msg_tx, mode, gui_panel, &publish).await;
        }
        Ok(_) => { /* 其他事件略過 */ }
        Err(e) => {
            output_push(
                msg_tx,
                mode,
                gui_panel,
                Error,
                format!("❌ Event loop error: {e:?}"),
            )
            .await;
            return true;
        }
    }

    false
}

async fn process_event_publish(
    msg_tx: &Sender<Msg>,
    mode: &Mode,
    gui_panel: &Option<String>,
    publish: &Publish,
) {
    let topic = &publish.topic;
    let re =
        regex::Regex::new(&format!(r"^{TOPIC_PREFIX}/([^/]+)/([^/]+)$")).expect("Failed to regex");

    if let Some(captures) = re.captures(topic) {
        let name = &captures[1];
        let key = &captures[2];
        let payload = String::from_utf8_lossy(&publish.payload);

        match key.parse::<DeviceKey>() {
            Ok(
                DeviceKey::Onboard
                | DeviceKey::Version
                | DeviceKey::TailscaleIp
                | DeviceKey::Temperature
                | DeviceKey::AppUptime,
            ) => {
                output_push(
                    msg_tx,
                    mode,
                    gui_panel,
                    Info,
                    format!("📩 pub:: {key} {name} {payload}"),
                )
                .await;

                msgs::cmd(
                    msg_tx,
                    MODULE,
                    &format!(
                        "{} {} {} {key} {name} {payload}",
                        consts::P,
                        plugin_devices::MODULE,
                        Action::Update,
                    ),
                )
                .await;
            }
            _ => {
                output_push(
                    msg_tx,
                    mode,
                    gui_panel,
                    Error,
                    format!("📩 pub:: {key} {name} {payload}"),
                )
                .await;
            }
        }
    }
}

async fn output_push(
    msg_tx: &Sender<Msg>,
    mode: &Mode,
    gui_panel: &Option<String>,
    level: log::Level,
    msg: String,
) {
    let ts = utils::time::ts();
    match mode {
        Mode::Gui => {
            msgs::cmd(
                msg_tx,
                MODULE,
                &format!(
                    "{} {} {} {} '{} [{level}] {msg}'",
                    consts::P,
                    plugin_panels::MODULE,
                    Action::OutputPush,
                    gui_panel.as_ref().unwrap(),
                    utils::time::ts_str(ts)
                ),
            )
            .await;
        }
        Mode::Cli => match level {
            Info => msgs::info(msg_tx, MODULE, &msg).await,
            Warn => msgs::warn(msg_tx, MODULE, &msg).await,
            Error => msgs::error(msg_tx, MODULE, &msg).await,
            _ => msgs::error(msg_tx, MODULE, &msg).await,
        },
    }
}
