use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc::Sender;
use walkdir::WalkDir;

use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::{api, common, ffmpeg, yt_dlp};

pub const MODULE: &str = "music";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    is_available: bool,
    yt_dlp: yt_dlp::YtDlp,
    ffmpeg: ffmpeg::Ffmpeg,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let mut myself = Self {
            msg_tx: msg_tx.clone(),
            is_available: false,
            yt_dlp: yt_dlp::YtDlp::new(msg_tx.clone(), consts::NAS_MUSIC_FOLDER).await?,
            ffmpeg: ffmpeg::Ffmpeg::new(msg_tx.clone()).await?,
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn init(&mut self) {
        self.info(consts::INIT.to_string()).await;
        self.is_available = self.yt_dlp.is_available && self.ffmpeg.is_available;
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  available: {}", self.is_available))
            .await;
        self.yt_dlp.handle_cmd_show().await;
        self.ffmpeg.handle_cmd_show().await;
    }

    async fn handle_cmd_download(&mut self, cmd_parts: &[String]) {
        self.info(Action::Show.to_string()).await;

        if !self.is_available {
            self.warn("Not available".to_string()).await;
            return;
        }

        if let Some(url) = cmd_parts.get(3) {
            self.yt_dlp.download(url).await;
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<url>",
                Action::Download.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {} <url>", Action::Download)).await;
        self.info("    url: the URL to download".to_string()).await;
        self.info(format!("  {}", Action::Upload)).await;
        self.info(format!("  {}", Action::Remove)).await;
        self.info("  Normal procedure:".to_string()).await;
        self.info("    1. Download music files from URL to NAS_MUSIC_FOLDER".to_string())
            .await;
        self.info("    2. Upload music files from NAS_MUSIC_FOLDER to server".to_string())
            .await;
        self.info("    3. Remove music files from NAS_MUSIC_FOLDER".to_string())
            .await;
    }

    async fn handle_cmd_upload(&self) {
        self.info(Action::Upload.to_string()).await;

        if globals::get_server_ip().is_none() {
            self.warn("Server IP is not set. Please set it first.".to_string())
                .await;
            return;
        }

        let server_ip = globals::get_server_ip().unwrap();

        let source_dir = Path::new(consts::NAS_MUSIC_FOLDER);
        let target_dir = Path::new(consts::NAS_UPLOAD_FOLDER);

        // for all files in consts::NAS_MUSIC_FOLDER to send upload request
        let msg_tx_clone = self.msg_tx.clone();
        tokio::task::spawn(async move {
            for entry in WalkDir::new(source_dir)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
            {
                let source_path = PathBuf::from(entry.path());
                let source_path_no_prefix = source_path.strip_prefix(source_dir).unwrap();
                let bytes = fs::read(&source_path).unwrap();
                let target_path = target_dir.join(source_path_no_prefix);

                let encoded = general_purpose::STANDARD.encode(&bytes);
                let mtime = fs::metadata(&source_path)
                    .and_then(|meta| meta.modified())
                    .map(|time| DateTime::<Utc>::from(time).to_rfc3339())
                    .unwrap_or_else(|_| Utc::now().to_rfc3339());

                let target_path = target_path.clone();
                let server_ip = server_ip.clone();
                let msg_tx_clone = msg_tx_clone.clone();
                tokio::task::spawn(async move {
                    api::post_upload(
                        &msg_tx_clone,
                        MODULE,
                        server_ip.as_str(),
                        &api::UploadRequest {
                            data: api::UploadData {
                                filename: target_path.to_string_lossy().to_string(),
                                content: encoded,
                                mtime,
                            },
                        },
                    )
                    .await;
                });
            }
        });
    }

    async fn handle_cmd_remove(&self) {
        self.info(Action::Remove.to_string()).await;

        // remove all files in consts::NAS_MUSIC_FOLDER
        let folder_path = Path::new(consts::NAS_MUSIC_FOLDER);
        let _ = fs::remove_dir_all(folder_path);
        let _ = fs::create_dir_all(folder_path);
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
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            Action::Download => self.handle_cmd_download(&cmd_parts).await,
            Action::Upload => self.handle_cmd_upload().await,
            Action::Remove => self.handle_cmd_remove().await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
