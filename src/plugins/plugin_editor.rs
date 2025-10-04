use std::fs::{self, File, OpenOptions};
use std::io::Read;

use anyhow::Result;
use async_trait::async_trait;
use ratatui::{
    Frame,
    crossterm::{cursor::SetCursorStyle, execute},
    layout::Position,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::globals;
use crate::messages::{Action, Key, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::{api, common, nas, panel};

pub const MODULE: &str = "editor";

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    file_name: String,
    output: Vec<String>,
    panel_info: panel::PanelInfo,
    cursor_position: (u16, u16), // (x, y)
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx,
            file_name: String::new(),
            output: vec![],
            panel_info: panel::PanelInfo {
                panel_type: panel::PanelType::Normal,
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            cursor_position: (0, 0),
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
        self.info(format!(
            "  {} <file_name> - Open a file in the editor",
            Action::Open
        ))
        .await;
        self.info(format!("  {} - Sync files with the server", Action::Sync))
            .await;
        self.info(format!("  {} <file_name> - Remove a file", Action::Remove))
            .await;
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

        self.output = buffer.lines().map(String::from).collect();
        self.cursor_position = (0, 0);

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

    async fn handle_action_key_home(&mut self) {
        self.cursor_position.0 = 0;
    }
    async fn handle_action_key_end(&mut self) {
        self.cursor_position.0 = self.output[self.cursor_position.1 as usize].len() as u16;
    }

    async fn handle_action_key_up(&mut self) {
        if self.cursor_position.1 > 0 {
            self.cursor_position.1 -= 1;
            let line_length = self.output[self.cursor_position.1 as usize].len() as u16;
            if self.cursor_position.0 > line_length {
                self.cursor_position.0 = line_length;
            }
        } else {
            self.cursor_position.0 = 0;
        }
    }

    async fn handle_action_key_down(&mut self) {
        if (self.cursor_position.1 as usize) < self.output.len() - 1 {
            //TODO: -1?
            self.cursor_position.1 += 1;
            let line_length = self.output[self.cursor_position.1 as usize].len() as u16;
            if self.cursor_position.0 > line_length {
                self.cursor_position.0 = line_length;
            }
        } else {
            self.cursor_position.0 = self.output[self.cursor_position.1 as usize].len() as u16;
        }
    }

    async fn handle_action_key_left(&mut self) {
        if self.cursor_position.0 > 0 {
            self.cursor_position.0 -= 1;
        } else if self.cursor_position.1 > 0 {
            self.cursor_position.1 -= 1;
            self.cursor_position.0 = self.output[self.cursor_position.1 as usize].len() as u16;
        }
    }

    async fn handle_action_key_right(&mut self) {
        let line_length = self.output[self.cursor_position.1 as usize].len() as u16;
        if self.cursor_position.0 < line_length {
            self.cursor_position.0 += 1;
        } else if (self.cursor_position.1 as usize) < self.output.len() - 1 {
            self.cursor_position.1 += 1;
            self.cursor_position.0 = 0;
        }
    }

    async fn handle_action_key(&mut self, cmd_parts: &[String]) {
        if let Some(key) = cmd_parts.get(3) {
            #[allow(clippy::single_match)]
            match key.parse::<Key>() {
                Ok(Key::ControlX) => self.handle_action_key_control_x().await,
                Ok(Key::Home) => self.handle_action_key_home().await,
                Ok(Key::End) => self.handle_action_key_end().await,
                Ok(Key::Up) => self.handle_action_key_up().await,
                Ok(Key::Down) => self.handle_action_key_down().await,
                Ok(Key::Left) => self.handle_action_key_left().await,
                Ok(Key::Right) => self.handle_action_key_right().await,
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

    async fn sync(&mut self) {
        // get folder_meta from server first
        if let Some(server_ip) = globals::get_server_ip() {
            let remote_folder_meta = match api::post_get_folder_meta(
                &self.msg_tx,
                MODULE,
                &server_ip,
                &api::GetFolderMetaRequest {
                    foldername: consts::NAS_EDITOR_FOLDER
                        .trim_start_matches('/')
                        .to_string(),
                },
            )
            .await
            {
                Ok(fm) => fm,
                Err(e) => {
                    self.warn(format!("Failed to get folder meta from server: {e}"))
                        .await;
                    return;
                }
            };

            let local_folder_meta = nas::get_folder_meta(consts::NAS_EDITOR_FOLDER);

            // if folder is the same
            if remote_folder_meta.hash == local_folder_meta.hash {
                self.info("Server == Local".to_string()).await;
                return;
            }

            // upload changed files
            for local_file in &local_folder_meta.files {
                let remote_file = remote_folder_meta
                    .files
                    .iter()
                    .find(|f| f.filename == local_file.filename);

                let file_path = format!("{}/{}", consts::NAS_EDITOR_FOLDER, local_file.filename);

                match remote_file {
                    None => {
                        self.info(format!("Server <= Local: `{}`", local_file.filename))
                            .await;
                        api::upload_file(&self.msg_tx, MODULE, &server_ip, &file_path, &file_path)
                            .await;
                    }
                    Some(remote_file) => {
                        if remote_file.hash != local_file.hash {
                            if remote_file.mtime < local_file.mtime {
                                self.info(format!("Server <= Local: `{}`", local_file.filename))
                                    .await;

                                api::upload_file(
                                    &self.msg_tx,
                                    MODULE,
                                    &server_ip,
                                    &file_path,
                                    &file_path,
                                )
                                .await;
                            } else {
                                self.info(format!("Server => Local: `{}`", local_file.filename))
                                    .await;

                                api::download_file(&self.msg_tx, MODULE, &server_ip, &file_path)
                                    .await;
                            }
                        } else {
                            self.info(format!("Server == Local: `{}`", local_file.filename))
                                .await;
                        }
                    }
                }
            }

            for remote_file in &remote_folder_meta.files {
                let local_file = local_folder_meta
                    .files
                    .iter()
                    .find(|f| f.filename == remote_file.filename);
                if local_file.is_none() {
                    let file_path =
                        format!("{}/{}", consts::NAS_EDITOR_FOLDER, remote_file.filename);

                    self.info(format!("Server => Local: `{}`", remote_file.filename))
                        .await;
                    api::download_file(&self.msg_tx, MODULE, &server_ip, &file_path).await;
                }
            }
        } else {
            self.warn(consts::SERVER_IP_NOT_SET.to_string()).await;
        }
    }

    async fn handle_action_sync(&mut self) {
        self.info(Action::Sync.to_string()).await;
        self.sync().await;
    }

    async fn handle_action_remove(&mut self, cmd_parts: &[String]) {
        self.info(Action::Remove.to_string()).await;

        let file_name = match cmd_parts.get(3) {
            Some(name) => name,
            None => {
                self.warn(common::MsgTemplate::MissingParameters.format(
                    "<file_name>",
                    Action::Remove.as_ref(),
                    &cmd_parts.join(" "),
                ))
                .await;
                return;
            }
        };

        let full_path = format!("{}/{}", consts::NAS_EDITOR_FOLDER, file_name);

        if fs::remove_file(&full_path).is_err() {
            self.warn(format!("Failed to remove file `{full_path}`"))
                .await;
            return;
        }

        self.info(format!("File `{full_path}` removed")).await;

        api::post_remove(
            &self.msg_tx,
            MODULE,
            &globals::get_server_ip().unwrap_or_default(),
            &api::RemoveRequest {
                filename: full_path.trim_start_matches('/').to_string(),
            },
        )
        .await;
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

        let scroll_offset = if self.output.len() as u16 > (area_height - 3) {
            self.output.len() as u16 - (area_height - 3)
        } else {
            0
        };

        let lines: Vec<Line> = self
            .output
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

        // cursor for popup panel
        let mut stdout = std::io::stdout();
        execute!(stdout, SetCursorStyle::DefaultUserShape).unwrap();

        frame.set_cursor_position(Position::new(
            panel_x + self.cursor_position.0 + 1,
            panel_y + self.cursor_position.1 + 1,
        ));
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
            Action::Open => self.handle_action_open(cmd_parts).await,
            Action::Key => self.handle_action_key(cmd_parts).await,
            Action::Sync => self.handle_action_sync().await,
            Action::Remove => self.handle_action_remove(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
