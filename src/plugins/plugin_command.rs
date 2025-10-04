use anyhow::Result;
use async_trait::async_trait;
use ratatui::{
    Frame,
    layout::Position,
    style::{Color, Style},
    text::Text,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, Msg};
use crate::plugins::{
    plugin_gui,
    plugins_main::{self, Plugin},
};
use crate::utils::{self, common, panel};

pub const MODULE: &str = "command";

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    output: String,
    panel_info: panel::PanelInfo,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx: msg_tx.clone(),
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

        let msg_tx_clone = msg_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                msgs::cmd(
                    &msg_tx_clone,
                    MODULE,
                    &format!("{} {} {}", consts::P, plugins_main::MODULE, Action::Redraw,),
                )
                .await;
            }
        });

        Ok(myself)
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
    }

    async fn handle_action_key(&mut self, cmd_parts: &[String]) {
        if let Some(key) = cmd_parts.get(3) {
            self.cmd(format!(
                "{} {} {} {key}",
                consts::P,
                plugin_gui::MODULE,
                Action::Key,
            ))
            .await;
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<key>",
                Action::Key.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_action_output_update(&mut self, cmd_parts: &[String]) {
        if let Some(output) = cmd_parts.get(3) {
            self.output = output.clone();
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

    fn draw(&mut self, frame: &mut Frame, active: bool) {
        let width = frame.area().width;
        let height = frame.area().height - 3;

        // Clear the panel area
        let (panel_x, panel_y, panel_width, panel_height) = (0, height, width, 3);

        let panel_area =
            panel::panel_rect(panel_x, panel_y, panel_width, panel_height, frame.area());
        frame.render_widget(Clear, panel_area);

        // Draw the panel block
        let sys_name = globals::get_sys_name();
        let version = env!("CARGO_PKG_VERSION");
        let sub_title = format!(
            "{sys_name} (v{version}) - {}",
            utils::time::ts_str(utils::time::ts())
        );

        let panel_block = Block::default()
            .borders(Borders::ALL)
            .title(format!("{MODULE} - {sub_title}"))
            .padding(ratatui::widgets::Padding::new(0, 0, 0, 0))
            .border_type(if active {
                BorderType::Double
            } else {
                BorderType::Plain
            })
            .style(Style::default().fg(if active { Color::Cyan } else { Color::White }));
        frame.render_widget(panel_block.clone(), panel_area);

        // Draw the panel content
        let text = Paragraph::new(Text::from(self.output.as_str()))
            .style(Style::default().fg(if active { Color::Cyan } else { Color::White }));
        frame.render_widget(text, panel_block.inner(panel_area));

        // cursor for panel command
        frame.set_cursor_position(Position::new(
            panel_x + self.output.len() as u16 + 1,
            panel_y + 1,
        ));
    }

    async fn handle_action(&mut self, action: Action, cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Key => self.handle_action_key(cmd_parts).await,
            Action::Gui => {
                if let Ok(panel_info) = self.handle_action_gui(cmd_parts).await {
                    self.panel_info = panel_info;
                }
            }
            Action::OutputUpdate => self.handle_action_output_update(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
