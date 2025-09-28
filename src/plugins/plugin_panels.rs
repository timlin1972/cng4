use anyhow::Result;
use async_trait::async_trait;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::{cursor::SetCursorStyle, execute},
    layout::{Position, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use tokio::sync::{broadcast, mpsc};

use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::common;

pub const MODULE: &str = "panels";
const CURSOR_PANEL_TITLE: &str = "command";
const MAX_OUTPUT_LEN: usize = 300;

#[derive(Debug)]
struct Panel {
    title: String,
    sub_title: String,
    plugin_name: String,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    output: Vec<String>,
}

#[derive(Debug)]
pub struct Plugin {
    msg_tx: mpsc::Sender<Msg>,
    shutdown_tx: broadcast::Sender<()>,
    terminal: Option<DefaultTerminal>,
    panels: Vec<Panel>,
    active_panel: usize,
}

impl Plugin {
    pub async fn new(
        msg_tx: mpsc::Sender<Msg>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Result<Self> {
        let mut myself = Self {
            msg_tx,
            shutdown_tx,
            terminal: None,
            panels: Vec::new(),
            active_panel: 0,
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn init(&mut self) {
        self.info(consts::INIT.to_string()).await;

        self.terminal = Some(ratatui::init());

        let mut stdout = std::io::stdout();
        execute!(stdout, SetCursorStyle::BlinkingBlock).unwrap();

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        tokio::spawn(async move {
            let _ = shutdown_rx.recv().await;

            let mut stdout = std::io::stdout();
            execute!(stdout, SetCursorStyle::DefaultUserShape).unwrap();

            ratatui::restore();
        });
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Number of panels: {}", self.panels.len()))
            .await;
        self.info(format!(
            "  {:<2} {:<12} {:<12} {:12}",
            "ID", "Title", "Subtitle", "Plugin"
        ))
        .await;
        for (idx, panel) in self.panels.iter().enumerate() {
            self.info(format!(
                "  {:<2} {:<12} {:<12} {:12}",
                idx, panel.title, panel.sub_title, panel.plugin_name
            ))
            .await;
        }
        self.info(format!("  Active panel: {}", self.active_panel))
            .await;
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
    }

    fn draw(&mut self, frame: &mut Frame) {
        for (idx, panel) in self.panels.iter_mut().enumerate() {
            draw_panel(panel, frame, idx == self.active_panel);
        }
    }

    async fn handle_cmd_create(&mut self, cmd_parts: Vec<String>) {
        let mut terminal = self.terminal.take().unwrap();

        if let (Some(title), Some(plugin_name), Some(x), Some(y), Some(width), Some(height)) = (
            cmd_parts.get(3),
            cmd_parts.get(4),
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
            cmd_parts.get(8),
        ) {
            let panel = Panel {
                title: title.to_string(),
                sub_title: String::new(),
                plugin_name: plugin_name.to_string(),
                x: x.parse::<u16>()
                    .unwrap_or_else(|_| panic!("Failed to parse x (`{x}`)")),
                y: y.parse::<u16>()
                    .unwrap_or_else(|_| panic!("Failed to parse y (`{y}`)")),
                width: width
                    .parse::<u16>()
                    .unwrap_or_else(|_| panic!("Failed to parse width (`{width}`)")),
                height: height
                    .parse::<u16>()
                    .unwrap_or_else(|_| panic!("Failed to parse height (`{height}`)")),
                output: vec![],
            };
            self.panels.push(panel);

            let _ = terminal.draw(|frame| self.draw(frame));
        } else {
            self.warn(format!(
                "Incomplete {} command: {cmd_parts:?}",
                Action::Create
            ))
            .await;
        }
        self.terminal = Some(terminal);
    }

    async fn handle_cmd_push(&mut self, cmd_parts: Vec<String>) {
        let mut terminal = self.terminal.take().unwrap();

        if let (Some(panel_title), Some(message)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            if let Some(panel) = self.panels.iter_mut().find(|p| p.title == *panel_title) {
                panel.output.push(message.to_string());
                let panel_output_len = panel.output.len();
                if panel_output_len > MAX_OUTPUT_LEN {
                    panel.output.drain(..panel_output_len - MAX_OUTPUT_LEN);
                }
            }
            let _ = terminal.draw(|frame| self.draw(frame));
        } else {
            self.warn(format!(
                "Incomplete {} command: {cmd_parts:?}",
                Action::Push
            ))
            .await;
        }

        self.terminal = Some(terminal);
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
            Action::Create => self.handle_cmd_create(cmd_parts).await,
            Action::Push => self.handle_cmd_push(cmd_parts).await,
            _ => self.warn(format!("Unsupported action: {action}")).await,
        }
    }
}

//
// helper functions
//

fn draw_panel(panel: &mut Panel, frame: &mut Frame, active: bool) {
    let width = frame.area().width;
    let height = frame.area().height - 3;

    let (panel_x, panel_y, panel_width, panel_height) = if panel.title == CURSOR_PANEL_TITLE {
        (0, height, width, 3)
    } else {
        (
            (width as f32 * panel.x as f32 / 100.0).round() as u16,
            (height as f32 * panel.y as f32 / 100.0).round() as u16,
            (width as f32 * panel.width as f32 / 100.0).round() as u16,
            (height as f32 * panel.height as f32 / 100.0).round() as u16,
        )
    };

    let panel_area = panel_rect(panel_x, panel_y, panel_width, panel_height, frame.area());
    frame.render_widget(Clear, panel_area);

    let panel_block = Block::default()
        .borders(Borders::ALL)
        .title(format!("{}{}", panel.title, panel.sub_title))
        .padding(ratatui::widgets::Padding::new(0, 0, 0, 0))
        .border_type(if active {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .style(Style::default().fg(if active { Color::Cyan } else { Color::White }));

    frame.render_widget(panel_block.clone(), panel_area);

    let area_height = panel_area.height;

    let scroll_offset =
        if panel.title != CURSOR_PANEL_TITLE && panel.output.len() as u16 > (area_height - 3) {
            panel.output.len() as u16 - (area_height - 3)
        } else {
            0
        };

    let lines: Vec<Line> = panel
        .output
        .iter()
        .flat_map(|entry| {
            entry
                .split('\n') // 處理內部的換行
                .map(|subline| {
                    if subline.contains("[WARN]") {
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

    // let text = Paragraph::new(Text::from(panel.output.join("\n")))
    let text = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(if active { Color::Cyan } else { Color::White }))
        .scroll((scroll_offset, 0));

    frame.render_widget(text, panel_block.inner(panel_area));

    // cursor is only for panel command
    if panel.title == CURSOR_PANEL_TITLE && !panel.output.is_empty() {
        frame.set_cursor_position(Position::new(
            panel_x + panel.output[0].len() as u16 + 1,
            panel_y + 1,
        ));
    }
}

fn panel_rect(x: u16, y: u16, width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x.saturating_add(x);
    let y = area.y.saturating_add(y);
    let width = width.min(area.width.saturating_sub(x - area.x));
    let height = height.min(area.height.saturating_sub(y - area.y));
    Rect {
        x,
        y,
        width,
        height,
    }
}
