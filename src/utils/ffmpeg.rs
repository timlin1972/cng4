use anyhow::Result;
use tokio::process::Command;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Msg};

const MODULE: &str = "ffmpeg";
const BIN: &str = "ffmpeg";
const ARG_VERSION: &str = "-version";

#[derive(Debug)]
pub struct Ffmpeg {
    msg_tx: Sender<Msg>,
    pub is_available: bool,
    version: Option<String>,
}

impl Ffmpeg {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let mut myself = Self {
            msg_tx,
            is_available: false,
            version: None,
        };

        myself.info(consts::NEW.to_string()).await;

        myself.init().await?;
        Ok(myself)
    }

    async fn is_available(&self) -> bool {
        Command::new(BIN)
            .arg(ARG_VERSION)
            .output()
            .await
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    async fn get_version(&mut self) -> Option<String> {
        let output = Command::new(BIN).arg(ARG_VERSION).output().await.ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = stdout.lines().next() {
            // 通常第一行像這樣：ffmpeg version 4.4.2-0ubuntu0.22.04.1 ...
            if let Some(version) = first_line.split_whitespace().nth(2) {
                return Some(version.to_string());
            }
        }

        None
    }

    async fn init(&mut self) -> Result<()> {
        self.info(consts::INIT.to_string()).await;

        self.is_available = self.is_available().await;
        self.version = self.get_version().await;

        Ok(())
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    pub async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  available: {}", self.is_available))
            .await;
        self.info(format!(
            "  version: {}",
            self.version.as_ref().unwrap_or(&"N/A".into())
        ))
        .await;
    }
}
