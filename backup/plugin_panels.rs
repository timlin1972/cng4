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
use crate::messages::{self as msgs, Action, Data, Key, Msg};
use crate::plugins::plugins_main;
use crate::utils::common;

pub const MODULE: &str = "panels";
const MAX_OUTPUT_LEN: usize = 300;

#[derive(Debug)]
struct Panel {
    title: String,
    sub_title: String,
    popup: bool,
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
    active_popup: Option<usize>,
    popup_cursor: (u16, u16),
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
            active_popup: None,
            popup_cursor: (0, 0),
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
        self.info(format!("  Number of panels: {}", self.panels.len()))
            .await;
        self.info(format!(
            "  {:<2} {:<12} {:<30} {:<5} {:12}",
            "ID", "Title", "Subtitle", "Popup", "Plugin"
        ))
        .await;
        for (idx, panel) in self.panels.iter().enumerate() {
            self.info(format!(
                "  {:<2} {:<12} {:<30} {:<5} {:12}",
                idx, panel.title, panel.sub_title, panel.popup, panel.plugin_name
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
        // for normal inactive panels
        for (idx, panel) in self.panels.iter_mut().enumerate() {
            if !panel.popup && idx != self.active_panel {
                draw_panel(panel, frame, false, self.active_popup, self.popup_cursor);
            }
        }

        // for normal active panel
        for (idx, panel) in self.panels.iter_mut().enumerate() {
            if !panel.popup && idx == self.active_panel {
                draw_panel(panel, frame, true, self.active_popup, self.popup_cursor);
                break;
            }
        }

        // for popup panels
        if let Some(active_popup) = self.active_popup {
            for (idx, panel) in self.panels.iter_mut().enumerate() {
                if panel.popup && idx == active_popup {
                    draw_panel(panel, frame, true, self.active_popup, self.popup_cursor);
                    break;
                }
            }
        }
    }

    async fn handle_cmd_create(&mut self, cmd_parts: Vec<String>) {
        let mut terminal = self.terminal.take().unwrap();

        if let (
            Some(title),
            Some(plugin_name),
            Some(popup),
            Some(x),
            Some(y),
            Some(width),
            Some(height),
        ) = (
            cmd_parts.get(3),
            cmd_parts.get(4),
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
            cmd_parts.get(8),
            cmd_parts.get(9),
        ) {
            let panel = Panel {
                title: title.to_string(),
                sub_title: String::new(),
                popup: popup == "popup",
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
                "Incomplete `{}` command: {cmd_parts:?}",
                Action::Create
            ))
            .await;
        }
        self.terminal = Some(terminal);
    }

    async fn handle_cmd_output_push(&mut self, cmd_parts: Vec<String>) {
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
                "Incomplete `{}` command: {cmd_parts:?}",
                Action::OutputPush
            ))
            .await;
        }

        self.terminal = Some(terminal);
    }

    async fn handle_cmd_output_update(&mut self, cmd_parts: Vec<String>) {
        let mut terminal = self.terminal.take().unwrap();

        if let (Some(panel_title), Some(message)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            if let Some(panel) = self.panels.iter_mut().find(|p| p.title == *panel_title) {
                panel.output.clear();
                panel.output.push(message.to_string());
            }
            let _ = terminal.draw(|frame| self.draw(frame));
        } else {
            self.warn(format!(
                "Incomplete `{}` command: {cmd_parts:?}",
                Action::Update
            ))
            .await;
        }

        self.terminal = Some(terminal);
    }

    async fn handle_cmd_key_tab(&mut self) {
        let mut terminal = self.terminal.take().unwrap();

        if self.active_popup.is_none() {
            self.active_panel = (self.active_panel + 1) % self.panels.len();
            let _ = terminal.draw(|frame| self.draw(frame));
        }

        self.terminal = Some(terminal);
    }

    async fn handle_cmd_key_arrow(&self, key: Key) {
        for (idx, panel) in self.panels.iter().enumerate() {
            if idx == self.active_panel {
                self.cmd(format!(
                    "{} {} {} {key}",
                    consts::P,
                    panel.plugin_name,
                    Action::Key,
                ))
                .await;
                break;
            }
        }
    }

    async fn handle_cmd_key_alt_c(&mut self) {
        let mut terminal = self.terminal.take().unwrap();

        for (idx, panel) in self.panels.iter_mut().enumerate() {
            if idx == self.active_panel {
                panel.output.clear();
                break;
            }
        }

        let _ = terminal.draw(|frame| self.draw(frame));
        self.terminal = Some(terminal);
    }

    async fn handle_cmd_key_alt(&mut self, key: Key) {
        let mut terminal = self.terminal.take().unwrap();

        for (idx, panel) in self.panels.iter_mut().enumerate() {
            if idx == self.active_panel {
                match key {
                    Key::AltUp => {
                        if panel.y > 0 {
                            panel.y -= 1;
                        }
                    }
                    Key::AltDown => {
                        panel.y += 1;
                    }
                    Key::AltLeft => {
                        if panel.x > 0 {
                            panel.x -= 1;
                        }
                    }
                    Key::AltRight => {
                        panel.x += 1;
                    }
                    Key::AltW => {
                        if panel.height > 2 {
                            panel.height -= 1;
                        }
                    }
                    Key::AltS => {
                        panel.height += 1;
                    }
                    Key::AltA => {
                        if panel.width > 2 {
                            panel.width -= 1;
                        }
                    }
                    Key::AltD => {
                        panel.width += 1;
                    }
                    _ => (),
                }
            }
        }

        let _ = terminal.draw(|frame| self.draw(frame));
        self.terminal = Some(terminal);
    }

    async fn handle_cmd_key_control_x(&mut self, key: Key) {
        let mut terminal = self.terminal.take().unwrap();

        if self.active_popup.is_some() {
            if key == Key::ControlX {
                self.active_popup = None;
            }
        }

        let _ = terminal.draw(|frame| self.draw(frame));
        self.terminal = Some(terminal);
    }

    async fn handle_cmd_key(&mut self, cmd_parts: Vec<String>) {
        if let Some(key) = cmd_parts.get(3) {
            match key.parse::<Key>() {
                Ok(k @ Key::Up) | Ok(k @ Key::Down) | Ok(k @ Key::Left) | Ok(k @ Key::Right) => {
                    self.handle_cmd_key_arrow(k).await
                }
                Ok(Key::Tab) => self.handle_cmd_key_tab().await,
                Ok(Key::AltC) => self.handle_cmd_key_alt_c().await,
                Ok(k @ Key::AltW)
                | Ok(k @ Key::AltS)
                | Ok(k @ Key::AltA)
                | Ok(k @ Key::AltD)
                | Ok(k @ Key::AltUp)
                | Ok(k @ Key::AltDown)
                | Ok(k @ Key::AltLeft)
                | Ok(k @ Key::AltRight) => self.handle_cmd_key_alt(k).await,
                Ok(k @ Key::ControlX) => self.handle_cmd_key_control_x(k).await,
                _ => (),
            }
        }
    }

    async fn handle_cmd_sub_title(&mut self, cmd_parts: Vec<String>) {
        let mut terminal = self.terminal.take().unwrap();

        #[allow(clippy::collapsible_if)]
        if let (Some(panel_title), Some(sub_title)) = (cmd_parts.get(3), cmd_parts.get(4)) {
            if let Some(panel) = self.panels.iter_mut().find(|p| p.title == *panel_title) {
                panel.sub_title = sub_title.to_string();
            }
        }

        let _ = terminal.draw(|frame| self.draw(frame));
        self.terminal = Some(terminal);
    }

    async fn handle_cmd_popup(&mut self, cmd_parts: Vec<String>) {
        let mut terminal = self.terminal.take().unwrap();

        #[allow(clippy::collapsible_if)]
        if let Some(panel_title) = cmd_parts.get(3) {
            for (idx, panel) in self.panels.iter_mut().enumerate() {
                if panel.title == *panel_title && panel.popup {
                    self.active_popup = Some(idx);
                    break;
                }
            }
        }

        let _ = terminal.draw(|frame| self.draw(frame));
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
            Action::OutputPush => self.handle_cmd_output_push(cmd_parts).await,
            Action::OutputUpdate => self.handle_cmd_output_update(cmd_parts).await,
            Action::Key => self.handle_cmd_key(cmd_parts).await,
            Action::SubTitle => self.handle_cmd_sub_title(cmd_parts).await,
            Action::Popup => self.handle_cmd_popup(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}

//
// helper functions
//

fn draw_panel(
    panel: &mut Panel,
    frame: &mut Frame,
    active: bool,
    active_popup: Option<usize>,
    popup_cursor: (u16, u16),
) {
    let width = frame.area().width;
    let height = frame.area().height - 3;

    let (panel_x, panel_y, panel_width, panel_height) = if panel.title == consts::COMMAND {
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
        if panel.title != consts::COMMAND && panel.output.len() as u16 > (area_height - 3) {
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
                            Style::default().fg(Color::Yellow),
                        ))
                    } else if subline.contains("[ERROR]") {
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

    // cursor for panel command
    if panel.title == consts::COMMAND && !panel.output.is_empty() {
        frame.set_cursor_position(Position::new(
            panel_x + panel.output[0].len() as u16 + 1,
            panel_y + 1,
        ));
    }

    // cursor for popup panel
    if active_popup.is_some() {
        let mut stdout = std::io::stdout();
        execute!(stdout, SetCursorStyle::DefaultUserShape).unwrap();

        frame.set_cursor_position(Position::new(
            panel_x + popup_cursor.0 + 1,
            panel_y + popup_cursor.1 + 1,
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
