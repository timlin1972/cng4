use anyhow::Result;
use async_trait::async_trait;
use colored::*;
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::consts;
use crate::globals;
use crate::messages::{Action, Key, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::{api, common, panel, time};

pub const MODULE: &str = "log";
const LOG_CAPACITY: usize = 1000;

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    mode: Mode,
    dest: Option<String>,
    logs: Vec<String>,
    panel_info: panel::PanelInfo,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>, mode: Mode) -> Result<Self> {
        let myself = Self {
            msg_tx,
            mode,
            dest: None,
            logs: Vec::new(),
            panel_info: panel::PanelInfo::new(panel::PanelType::Normal),
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Mode: {}", self.mode)).await;
        self.info(format!("  Dest: {:?}", self.dest)).await;
        self.info(format!("  Panel info: {:?}", self.panel_info))
            .await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {} <dest>", Action::Dest)).await;
        self.info("    dest: the destination IP to send log messages to".to_string())
            .await;
    }

    async fn handle_action_log(&mut self, ts: u64, plugin: &str, cmd_parts: &[String]) {
        if let (Some(level), Some(msg)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            // if dest exists, send log to dest
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

            match self.mode {
                Mode::Gui => {
                    let msgs: Vec<&str> = msg.split('\n').collect();

                    for msg in msgs {
                        self.logs.push(format!(
                            "{} {plugin:>10}: [{}] {msg}",
                            time::ts_str(ts),
                            common::level_str(level)
                        ));
                    }
                    if self.logs.len() > LOG_CAPACITY {
                        self.logs.remove(0);
                    }
                    self.cmd(format!(
                        "{} {} {} {}",
                        consts::P,
                        plugins_main::MODULE,
                        Action::Redraw,
                        MODULE
                    ))
                    .await;
                }
                Mode::Cli => {
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
                }
            }
        } else {
            self.warn(format!("Incomplete log command: {cmd_parts:?}"))
                .await;
        }
    }

    async fn handle_action_dest(&mut self, cmd_parts: &[String]) {
        if let Some(dest) = cmd_parts.get(3) {
            self.dest = Some(dest.to_string());
        } else {
            self.dest = None;
        }
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

    async fn handle_action(&mut self, action: Action, cmd_parts: &[String], msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Log => self.handle_action_log(msg.ts, &msg.plugin, cmd_parts).await,
            Action::Gui => {
                if let Ok(panel_info) = self.handle_action_gui(cmd_parts).await {
                    self.panel_info = panel_info;
                }
            }
            Action::Dest => self.handle_action_dest(cmd_parts).await,
            Action::Key => self.handle_action_key(cmd_parts).await,
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
