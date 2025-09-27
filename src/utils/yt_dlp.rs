use std::path::Path;
use std::process::Stdio;

use anyhow::Result;
use tokio::process::Command;
use tokio::sync::mpsc::Sender;
use walkdir::WalkDir;

use crate::consts;
use crate::messages::{self as msgs, Action, Msg};

const MODULE: &str = "yt_dlp";
const BIN: &str = "yt-dlp";
const ARG_VERSION: &str = "--version";
const YT_DLP_CACHE: &str = "./yt_dlp_cache";
const URL_PREFIX: usize = 5;
const URL_SUFFIX: usize = 6;

#[derive(Debug)]
pub struct YtDlp {
    msg_tx: Sender<Msg>,
    output_dir: String,
    pub is_available: bool,
    version: Option<String>,
}

impl YtDlp {
    pub async fn new(msg_tx: Sender<Msg>, output_dir: &str) -> Result<Self> {
        let mut myself = Self {
            msg_tx,
            output_dir: output_dir.to_string(),
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

        let version = String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string();

        Some(version)
    }

    async fn init(&mut self) -> Result<()> {
        self.info(consts::INIT.to_string()).await;

        std::fs::create_dir_all(&self.output_dir)?;
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
        self.info(format!("  output_dir: `{}`", self.output_dir))
            .await;
    }

    pub async fn download(&mut self, url: &str) {
        self.info(format!("Downloading from `{}`...", shorten(url)))
            .await;

        let url = url.to_string();
        let msg_tx = self.msg_tx.clone();
        let output_dir = self.output_dir.clone();
        tokio::spawn(async move {
            match download(&url, &output_dir).await {
                Ok(_) => {
                    msgs::info(
                        &msg_tx,
                        MODULE,
                        &format!("Downloaded from `{}`", shorten(&url)),
                    )
                    .await
                }

                Err(e) => msgs::warn(&msg_tx, MODULE, &e.to_string()).await,
            }
        });
    }
}

//
// Helper functions
//

fn shorten(s: &str) -> String {
    let len = s.chars().count();

    if len <= URL_PREFIX + URL_SUFFIX {
        s.to_string()
    } else {
        let prefix: String = s.chars().take(URL_PREFIX).collect();
        let suffix: String = s
            .chars()
            .rev()
            .take(URL_SUFFIX)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{prefix}...{suffix}")
    }
}

fn remove_dir(remove_dir: &str) {
    let dir_to_remove = Path::new(remove_dir);

    if dir_to_remove.exists() {
        let _ = std::fs::remove_dir_all(dir_to_remove);
    }
}

async fn download(url: &str, output_dir: &str) -> Result<()> {
    // prepare cache dir
    remove_dir(YT_DLP_CACHE);
    let _ = std::fs::create_dir_all(YT_DLP_CACHE);

    let status = Command::new(BIN)
        .args([
            "--output",
            &format!("{YT_DLP_CACHE}/%(title)s.%(ext)s"),
            "--embed-thumbnail",
            "--add-metadata",
            "--extract-audio",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "320K",
            url,
        ])
        .stdout(Stdio::null()) // 隱藏標準輸出
        .stderr(Stdio::null()) // 隱藏錯誤輸出
        .status()
        .await;

    match status {
        Ok(status) if status.success() => {
            let _ = move_music(YT_DLP_CACHE, output_dir);
            remove_dir(YT_DLP_CACHE);
            Ok(())
        }
        Ok(_) => {
            remove_dir(YT_DLP_CACHE);
            Err(anyhow::anyhow!(
                "Failed to download from `{}`",
                shorten(url)
            ))
        }
        Err(_) => {
            remove_dir(YT_DLP_CACHE);
            Err(anyhow::anyhow!("Failed to execute `{BIN}` command"))
        }
    }
}

fn move_music(source_dir: &str, target_dir: &str) -> std::io::Result<()> {
    let source_dir = Path::new(source_dir);
    let target_dir = Path::new(target_dir);

    // 確保目標資料夾存在
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)?;
    }

    // 遍歷 source_dir 底下的所有檔案
    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let relative_path = entry.path().strip_prefix(source_dir).unwrap();
        let target_path = target_dir.join(relative_path);

        // 建立目標子資料夾（如果有）
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // 搬移檔案
        std::fs::rename(entry.path(), &target_path)?;
    }

    Ok(())
}
