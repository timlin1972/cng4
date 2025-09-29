use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::{common, ffmpeg, yt_dlp};

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
        if !self.is_available {
            self.warn("Not available".to_string()).await;
            return;
        }

        if let Some(url) = cmd_parts.get(3) {
            self.yt_dlp.download(url).await;
        } else {
            self.warn(format!("Missing URL for download command: `{cmd_parts:?}`"))
                .await;
        }
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {} <url>", Action::Download)).await;
        self.info("    url: the URL to download".to_string()).await;
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
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref()))
                    .await
            }
        }
    }
}
