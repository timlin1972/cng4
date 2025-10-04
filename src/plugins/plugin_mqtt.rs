use anyhow::Result;
use async_trait::async_trait;
use log::Level::{Error, Info, Warn};
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use rumqttc::{AsyncClient, Event, Incoming, LastWill, MqttOptions, Publish, QoS};
use tokio::sync::{broadcast, mpsc::Sender};

use crate::arguments::Mode;
use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, DeviceKey, Key, Msg};
use crate::plugins::{
    plugin_devices,
    plugins_main::{self, Plugin},
};
use crate::utils::{self, common, panel};

pub const MODULE: &str = "mqtt";
const BROKER: &str = "broker.emqx.io";
const BROKER_PORT: u16 = 1883;
const MQTT_KEEP_ALIVE: u64 = 300;
const RESTART_DELAY: u64 = 60;
const TOPIC_PREFIX: &str = "tln";
const MAX_OUTPUT_LEN: usize = 300;

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    shutdown_tx: broadcast::Sender<()>,
    mode: Mode,
    client: Option<AsyncClient>,
    logs: Vec<String>,
    panel_info: panel::PanelInfo,
}

impl PluginUnit {
    pub async fn new(
        msg_tx: Sender<Msg>,
        shutdown_tx: broadcast::Sender<()>,
        mode: Mode,
    ) -> Result<Self> {
        let myself = Self {
            msg_tx,
            shutdown_tx,
            mode,
            client: None,
            logs: vec![],
            panel_info: panel::PanelInfo::new(panel::PanelType::Normal),
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
        let mode_clone = self.mode.clone();
        let client_clone = client.clone();

        tokio::spawn(async move {
            msgs::info(&msg_tx_clone, MODULE, "5/5: Receive").await;

            let mut shoutdown_flag = false;
            loop {
                tokio::select! {
                    event = connection.poll() => {
                        if process_event(&msg_tx_clone, &mode_clone, event).await {
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

    async fn handle_action_restart(&mut self) {
        self.info(Action::Restart.to_string()).await;
        self.restart().await;
    }

    async fn handle_action_disconnected(&mut self) {
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
                        Info,
                        format!("📤 pub:: {key} {name} {payload}"),
                    )
                    .await;
                }
            }
        }
    }

    async fn handle_action_publish(&mut self, cmd_parts: &[String]) {
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
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<retain> <key> <payload>",
                Action::Publish.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Mode: {}", self.mode)).await;
        self.info(format!(
            "  MQTT Client connected: {}",
            self.client.is_some()
        ))
        .await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
    }

    async fn handle_action_key_alt_c(&mut self) {
        self.logs.clear();
        self.cmd(format!(
            "{} {} {} {}",
            consts::P,
            plugins_main::MODULE,
            Action::Redraw,
            MODULE
        ))
        .await;
    }

    async fn handle_action_key(&mut self, cmd_parts: &[String]) {
        if let Some(key) = cmd_parts.get(3) {
            match key.parse::<Key>() {
                Ok(_k @ Key::AltC) => self.handle_action_key_alt_c().await,
                Ok(k @ Key::AltUp)
                | Ok(k @ Key::AltDown)
                | Ok(k @ Key::AltLeft)
                | Ok(k @ Key::AltRight)
                | Ok(k @ Key::AltW)
                | Ok(k @ Key::AltS)
                | Ok(k @ Key::AltA)
                | Ok(k @ Key::AltD) => {
                    (
                        self.panel_info.x,
                        self.panel_info.y,
                        self.panel_info.w,
                        self.panel_info.h,
                    ) = self
                        .handle_action_key_position(
                            k,
                            self.panel_info.x,
                            self.panel_info.y,
                            self.panel_info.w,
                            self.panel_info.h,
                        )
                        .await;
                }
                _ => (),
            }
        }
    }

    async fn handle_action_output_push(&mut self, cmd_parts: &[String]) {
        if let Some(output) = cmd_parts.get(3) {
            self.logs.push(output.to_string());
            let logs_len = self.logs.len();
            if logs_len > MAX_OUTPUT_LEN {
                self.logs.drain(..logs_len - MAX_OUTPUT_LEN);
            }

            self.cmd(format!(
                "{} {} {}",
                consts::P,
                plugins_main::MODULE,
                Action::Redraw,
            ))
            .await;
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<output>",
                Action::OutputUpdate.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
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

    fn panel_info(&self) -> &panel::PanelInfo {
        &self.panel_info
    }

    async fn handle_action(&mut self, action: Action, cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Gui => {
                if let Ok(panel_info) = self.handle_action_gui(cmd_parts).await {
                    self.panel_info = panel_info;
                }
            }
            Action::Restart => self.handle_action_restart().await,
            Action::Disconnected => self.handle_action_disconnected().await,
            Action::Publish => self.handle_action_publish(cmd_parts).await,
            Action::Key => self.handle_action_key(cmd_parts).await,
            Action::OutputPush => self.handle_action_output_push(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame, active: bool) {
        // Clear the panel area
        let (panel_x, panel_y, panel_width, panel_height) = panel::caculate_position(
            frame,
            self.panel_info.x,
            self.panel_info.y,
            self.panel_info.w,
            self.panel_info.h,
        );

        let panel_area =
            panel::panel_rect(panel_x, panel_y, panel_width, panel_height, frame.area());
        frame.render_widget(Clear, panel_area);

        // Draw the panel block
        let panel_block = Block::default()
            .borders(Borders::ALL)
            .title(MODULE)
            .padding(ratatui::widgets::Padding::new(0, 0, 0, 0))
            .border_type(if active {
                BorderType::Double
            } else {
                BorderType::Plain
            })
            .style(Style::default().fg(if active { Color::Cyan } else { Color::White }));
        frame.render_widget(panel_block.clone(), panel_area);

        // Draw the panel content
        let area_height = panel_area.height;

        let scroll_offset = if self.logs.len() as u16 > (area_height - 3) {
            self.logs.len() as u16 - (area_height - 3)
        } else {
            0
        };

        let lines: Vec<Line> = self
            .logs
            .iter()
            .flat_map(|entry| {
                entry
                    .split('\n') // 處理內部的換行
                    .map(|subline| {
                        if subline.contains("[W]") {
                            Line::from(Span::styled(
                                subline.to_string(),
                                Style::default().fg(Color::Yellow),
                            ))
                        } else if subline.contains("[E]") {
                            Line::from(Span::styled(
                                subline.to_string(),
                                Style::default().fg(Color::Red),
                            ))
                        } else {
                            Line::from(Span::raw(subline.to_string()))
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let text = Paragraph::new(Text::from(lines))
            .style(Style::default().fg(if active { Color::Cyan } else { Color::White }))
            .scroll((scroll_offset, 0));

        frame.render_widget(text, panel_block.inner(panel_area));
    }
}

async fn process_event(
    msg_tx: &Sender<Msg>,
    mode: &Mode,
    event: Result<Event, rumqttc::ConnectionError>,
) -> bool {
    match event {
        Ok(Event::Incoming(Incoming::Publish(publish))) => {
            process_event_publish(msg_tx, mode, &publish).await;
        }
        Ok(_) => { /* 其他事件略過 */ }
        Err(e) => {
            output_push(msg_tx, mode, Error, format!("❌ Event loop error: {e:?}")).await;
            return true;
        }
    }

    false
}

async fn process_event_publish(msg_tx: &Sender<Msg>, mode: &Mode, publish: &Publish) {
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
                    Error,
                    format!("📩 pub:: {key} {name} {payload}"),
                )
                .await;
            }
        }
    }
}

async fn output_push(msg_tx: &Sender<Msg>, mode: &Mode, level: log::Level, msg: String) {
    let ts = utils::time::ts();
    match mode {
        Mode::Gui => {
            let msg = format!(
                "{} [{}] {msg}",
                utils::time::ts_str(ts),
                common::level_to_str(&level)
            );
            msgs::cmd(
                msg_tx,
                MODULE,
                &format!("{} {MODULE} {} '{msg}'", consts::P, Action::OutputPush,),
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
