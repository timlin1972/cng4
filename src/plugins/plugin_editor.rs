use std::fs::{self, File, OpenOptions};
use std::io::Read;

use anyhow::Result;
use async_trait::async_trait;
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{Action, Key, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::{common, panel};

pub const MODULE: &str = "editor";

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    file_name: String,
    output: String,
    panel_info: panel::PanelInfo,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx,
            file_name: String::new(),
            output: String::new(),
            panel_info: panel::PanelInfo {
                panel_type: panel::PanelType::Normal,
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;

        let _ = fs::create_dir_all(consts::NAS_EDITOR_FOLDER);
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;

        let files = common::list_files(consts::NAS_EDITOR_FOLDER);
        for file in files {
            self.info(file).await;
        }
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
    }

    async fn handle_action_gui(&mut self, cmd_parts: &[String]) {
        if let (Some(panel_type), Some(x), Some(y), Some(w), Some(h)) = (
            cmd_parts.get(3),
            cmd_parts.get(4),
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
        ) {
            let panel_type = panel_type.parse::<panel::PanelType>().unwrap();
            let x = x.parse::<u16>().unwrap();
            let y = y.parse::<u16>().unwrap();
            let w = w.parse::<u16>().unwrap();
            let h = h.parse::<u16>().unwrap();

            self.panel_info = panel::PanelInfo {
                panel_type,
                x,
                y,
                w,
                h,
            };

            self.cmd(format!(
                "{} {} {} {MODULE}",
                consts::P,
                plugins_main::MODULE,
                Action::InsertPanel,
            ))
            .await;
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<panel_type> <x> <y> <width> <height>",
                Action::Gui.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_action_open(&mut self, cmd_parts: &[String]) {
        self.info(Action::Open.to_string()).await;

        let file_name = match cmd_parts.get(3) {
            Some(name) => name,
            None => {
                self.warn(common::MsgTemplate::MissingParameters.format(
                    "<file_name>",
                    Action::Open.as_ref(),
                    &cmd_parts.join(" "),
                ))
                .await;
                return;
            }
        };

        self.file_name = file_name.to_string();

        self.cmd(format!(
            "{} {} {} {MODULE}",
            consts::P,
            plugins_main::MODULE,
            Action::Popup
        ))
        .await;

        let full_path = format!("{}/{}", consts::NAS_EDITOR_FOLDER, file_name);

        if let Err(err) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&full_path)
        {
            self.warn(format!("Failed to open file `{full_path}`: {err}"))
                .await;
            return;
        }

        let mut file = match File::open(&full_path) {
            Ok(file) => file,
            Err(err) => {
                self.warn(format!("Failed to open file `{full_path}`: {err}"))
                    .await;
                return;
            }
        };

        let mut buffer = String::new();
        if file.read_to_string(&mut buffer).is_err() {
            self.warn(format!("Failed to read file `{full_path}`"))
                .await;
            return;
        }

        self.output = buffer;

        self.cmd(format!(
            "{} {} {}",
            consts::P,
            plugins_main::MODULE,
            Action::Redraw,
        ))
        .await;
    }

    async fn handle_action_key_control_x(&mut self) {
        self.cmd(format!(
            "{} {} {}",
            consts::P,
            plugins_main::MODULE,
            Action::Popup
        ))
        .await;

        self.cmd(format!(
            "{} {} {}",
            consts::P,
            plugins_main::MODULE,
            Action::Redraw,
        ))
        .await;
    }

    async fn handle_action_key(&mut self, cmd_parts: &[String]) {
        if let Some(key) = cmd_parts.get(3) {
            #[allow(clippy::single_match)]
            match key.parse::<Key>() {
                Ok(Key::ControlX) => self.handle_action_key_control_x().await,
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
            .title(format!("{MODULE} - {}", self.file_name))
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

        let output = &self.output.lines().collect::<Vec<&str>>();

        let scroll_offset = if output.len() as u16 > (area_height - 3) {
            output.len() as u16 - (area_height - 3)
        } else {
            0
        };

        let lines: Vec<Line> = output
            .iter()
            .flat_map(|entry| {
                entry
                    .split('\n') // 處理內部的換行
                    .map(|subline| Line::from(Span::raw(subline.to_string())))
                    .collect::<Vec<_>>()
            })
            .collect();

        let text = Paragraph::new(Text::from(lines))
            .style(Style::default().fg(if active { Color::Cyan } else { Color::White }))
            .scroll((scroll_offset, 0));

        frame.render_widget(text, panel_block.inner(panel_area));
    }

    async fn handle_action(&mut self, action: Action, cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Gui => self.handle_action_gui(cmd_parts).await,
            Action::Open => self.handle_action_open(cmd_parts).await,
            Action::Key => self.handle_action_key(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
