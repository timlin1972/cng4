use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinSet;
use walkdir::WalkDir;

use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::{api, common, ffmpeg, nas, yt_dlp};

pub const MODULE: &str = "music";

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    is_available: bool,
    yt_dlp: yt_dlp::YtDlp,
    ffmpeg: ffmpeg::Ffmpeg,
}

impl PluginUnit {
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

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  available: {}", self.is_available))
            .await;
        self.yt_dlp.handle_action_show().await;
        self.ffmpeg.handle_action_show().await;

        let files = common::list_files(consts::NAS_MUSIC_FOLDER);
        for file in files {
            self.info(file).await;
        }

        let folder_meta = nas::get_folder_meta(consts::NAS_MUSIC_FOLDER);
        self.info(format!("  Folder meta: {}", folder_meta)).await;
    }

    async fn handle_action_download(&mut self, cmd_parts: &[String]) {
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

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
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

    async fn handle_action_upload(&self) {
        self.info(Action::Upload.to_string()).await;

        if globals::get_server_ip().is_none() {
            self.warn(consts::SERVER_IP_NOT_SET.to_string()).await;
            return;
        }

        let server_ip = globals::get_server_ip().unwrap();

        let source_dir = Path::new(consts::NAS_MUSIC_FOLDER);
        let target_dir = Path::new(consts::NAS_UPLOAD_FOLDER);

        // for all files in consts::NAS_MUSIC_FOLDER to send upload request
        let msg_tx_clone = self.msg_tx.clone();
        tokio::task::spawn(async move {
            let mut count = 0;
            let mut join_set = JoinSet::new();
            for entry in WalkDir::new(source_dir)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
            {
                let source_path = PathBuf::from(entry.path());
                let source_path_no_prefix = source_path.strip_prefix(source_dir).unwrap();
                let target_path = target_dir.join(source_path_no_prefix);

                let server_ip = server_ip.clone();
                let msg_tx_clone_clone = msg_tx_clone.clone();

                count += 1;
                let count_clone = count;

                join_set.spawn(async move {
                    let filename = target_path.to_string_lossy().to_string();

                    msgs::info(
                        &msg_tx_clone_clone,
                        MODULE,
                        &format!(
                            "  Uploading file #{count_clone}: `{}`",
                            common::shorten(&filename, 20, 0)
                        ),
                    )
                    .await;

                    api::upload_file(
                        &msg_tx_clone_clone,
                        MODULE,
                        server_ip.as_str(),
                        entry.path().to_str().unwrap(),
                        &filename,
                    )
                    .await;

                    msgs::info(
                        &msg_tx_clone_clone,
                        MODULE,
                        &format!(
                            "  Uploaded file #{count_clone}: `{}`",
                            common::shorten(&filename, 20, 0)
                        ),
                    )
                    .await;
                });
            }

            while let Some(res) = join_set.join_next().await {
                if let Err(e) = res {
                    msgs::warn(&msg_tx_clone, MODULE, &format!("A upload task failed: {e}")).await;
                }
            }
            msgs::info(
                &msg_tx_clone,
                MODULE,
                &format!("  Uploaded {count} (all) files done."),
            )
            .await;
        });
    }

    async fn handle_action_remove(&self) {
        self.info(Action::Remove.to_string()).await;

        // remove all files in consts::NAS_MUSIC_FOLDER
        let folder_path = Path::new(consts::NAS_MUSIC_FOLDER);
        let _ = fs::remove_dir_all(folder_path);
        let _ = fs::create_dir_all(folder_path);
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

    async fn handle_action(&mut self, action: Action, cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Download => self.handle_action_download(cmd_parts).await,
            Action::Upload => self.handle_action_upload().await,
            Action::Remove => self.handle_action_remove().await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
