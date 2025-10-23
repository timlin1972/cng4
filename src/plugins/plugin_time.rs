use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use figlet_rs::FIGfont;
use rand::Rng;
use ratatui::{
    Frame,
    crossterm::{cursor::SetCursorStyle, execute},
    style::{Color, Style},
    text::Text,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use tokio::sync::{Mutex, mpsc::Sender};

use crate::consts;
use crate::messages::{self as msgs, Action, Key, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::{self, common, panel};

pub const MODULE: &str = "time";

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    output: String,
    panel_info: panel::PanelInfo,
    open: Arc<Mutex<bool>>,
    escape_secs: u8,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx,
            output: String::new(),
            panel_info: panel::PanelInfo {
                panel_type: panel::PanelType::Normal,
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            open: Arc::new(Mutex::new(false)),
            escape_secs: 0,
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;

        self.info(format!("  {}", Action::Open)).await;
    }

    async fn handle_action_open(&mut self) {
        self.info(Action::Open.to_string()).await;

        self.cmd(format!(
            "{} {} {} {MODULE}",
            consts::P,
            plugins_main::MODULE,
            Action::Popup
        ))
        .await;

        let msg_tx = self.msg_tx.clone();
        self.open.lock().await.clone_from(&true);
        let open = Arc::clone(&self.open);
        tokio::spawn(async move {
            loop {
                let is_open = *open.lock().await;
                if !is_open {
                    break;
                }
                let current_time = utils::time::ts_str(utils::time::ts());
                output_update(&msg_tx, &current_time).await;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    async fn handle_action_key_control_x(&mut self) {
        self.open.lock().await.clone_from(&false);

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
                Ok(k @ Key::AltUp)
                | Ok(k @ Key::AltDown)
                | Ok(k @ Key::AltLeft)
                | Ok(k @ Key::AltRight) => {
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

    async fn handle_action_output_update(&mut self, cmd_parts: &[String]) {
        if let Some(output) = cmd_parts.get(3) {
            self.output = output.clone();

            self.escape_secs += 1;

            let escape_secs_change = {
                let mut rng = rand::rng();
                rng.random_range(30..=60)
            };

            if self.escape_secs >= escape_secs_change {
                let (new_x, new_y) = {
                    let mut rng = rand::rng();
                    (rng.random_range(0..=60), rng.random_range(0..=60))
                };

                self.panel_info.x = new_x;
                self.panel_info.y = new_y;
                self.escape_secs = 0;
            }

            self.cmd(format!(
                "{} {} {}",
                consts::P,
                plugins_main::MODULE,
                Action::Redraw,
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
        // Clear the panel area
        #[allow(unused_assignments)]
        let (panel_x, panel_y, mut panel_width, mut panel_height) = panel::caculate_position(
            frame,
            self.panel_info.x,
            self.panel_info.y,
            self.panel_info.w,
            self.panel_info.h,
        );

        panel_width = 65;
        panel_height = 8;

        let panel_area =
            panel::panel_rect(panel_x, panel_y, panel_width, panel_height, frame.area());
        frame.render_widget(Clear, panel_area);

        // Draw the panel block
        let panel_block = Block::default()
            .borders(Borders::ALL)
            .title(MODULE.to_string())
            .padding(ratatui::widgets::Padding::new(0, 0, 0, 0))
            .border_type(if active {
                BorderType::Double
            } else {
                BorderType::Plain
            })
            .style(Style::default().fg(if active { Color::Cyan } else { Color::White }));
        frame.render_widget(panel_block.clone(), panel_area);

        // Draw the panel content
        let scroll_offset = 0;

        let text = Paragraph::new(Text::from(big_clock(self.output.as_str())))
            .style(Style::default().fg(if active { Color::Cyan } else { Color::White }))
            .scroll((scroll_offset, 0));

        frame.render_widget(text, panel_block.inner(panel_area));

        // cursor for popup panel
        let mut stdout = std::io::stdout();
        execute!(stdout, SetCursorStyle::DefaultUserShape).unwrap();
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
            Action::Open => self.handle_action_open().await,
            Action::Key => self.handle_action_key(cmd_parts).await,
            Action::OutputUpdate => self.handle_action_output_update(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}

async fn output_update(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::cmd(
        msg_tx,
        MODULE,
        &format!("{} {MODULE} {} '{msg}'", consts::P, Action::OutputUpdate,),
    )
    .await;
}

// fn big_clock(str: &str) -> String {
//     let standard_font = FIGfont::standard().unwrap();
//     match standard_font.convert(str) {
//         Some(figure) => {}
//         None => String::from("N/A"),
//     }
// }

fn big_clock(input: &str) -> String {
    let font = FIGfont::standard().unwrap();
    let digit_width = 9;
    let colon_width = 3;

    let chars: Vec<char> = input.chars().collect();
    let mut glyphs: Vec<Vec<String>> = vec![];

    for (i, ch) in chars.iter().enumerate() {
        // 判斷是否為十位數（靠右）或個位數（靠左）
        let is_tens = i == 0 || i == 3 || i == 6;
        let is_colon = *ch == ':';
        let width = if is_colon { colon_width } else { digit_width };

        if let Some(fig) = font.convert(&ch.to_string()) {
            let lines: Vec<String> = fig
                .to_string()
                .lines()
                .map(|line| {
                    if is_colon {
                        format!("{:<width$}", line, width = width)
                    } else if is_tens {
                        format!("{:>width$}", line, width = width)
                    } else {
                        format!("{:<width$}", line, width = width)
                    }
                })
                .collect();
            glyphs.push(lines);
        }
    }

    let line_count = glyphs.first().map_or(0, |g| g.len());
    let mut lines = vec![String::new(); line_count];

    for i in 0..line_count {
        for glyph in &glyphs {
            lines[i] += &glyph[i];
        }
    }

    lines.join("\n")
}
