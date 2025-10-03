use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Msg};
use crate::utils::{self, common};

#[derive(Deserialize, Serialize, Debug)]
pub struct CmdRequest {
    pub cmd: String,
}

impl fmt::Display for CmdRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cmd)
    }
}

pub async fn post_cmd(
    msg_tx: &Sender<Msg>,
    device_name: &str,
    module: &str,
    ip: &str,
    cmd: &CmdRequest,
) {
    msgs::info(msg_tx, module, &format!("-> `{device_name}`: `{cmd}`")).await;

    let client = reqwest::Client::new();
    let ret = client
        .post(format!("http://{ip}:{}/{}", consts::WEB_PORT, Action::Cmd))
        .json(cmd)
        .send()
        .await;

    match ret {
        Ok(response) => {
            if response.status().is_success() {
                let text = response.text().await.unwrap_or_default();
                msgs::info(
                    msg_tx,
                    module,
                    &format!("<- `{device_name}`: `{cmd}`: `{text}`"),
                )
                .await;
            } else {
                msgs::warn(
                    msg_tx,
                    module,
                    &format!(
                        "Failed to post cmd to {ip} `{cmd}`: HTTP {}",
                        response.status()
                    ),
                )
                .await;
            }
        }
        Err(e) => {
            msgs::warn(
                msg_tx,
                module,
                &format!("Error posting cmd to {ip} `{cmd}`: {e}"),
            )
            .await;
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct UploadData {
    pub filename: String,
    pub content: String,
    pub mtime: String,
}

#[derive(Deserialize, Serialize)]
pub struct UploadRequest {
    pub data: UploadData,
}

impl fmt::Display for UploadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", common::shorten(&self.data.filename, 20, 0))
    }
}

pub async fn post_upload(msg_tx: &Sender<Msg>, module: &str, ip: &str, upload: &UploadRequest) {
    msgs::info(msg_tx, module, &format!("POST /upload to {ip} `{upload}`")).await;
    let client = reqwest::Client::new();
    let ret = client
        .post(format!(
            "http://{ip}:{}/{}",
            consts::WEB_PORT,
            Action::Upload
        ))
        .json(upload)
        .send()
        .await;

    match ret {
        Ok(response) => {
            if response.status().is_success() {
                let text = response.text().await.unwrap_or_default();
                msgs::info(
                    msg_tx,
                    module,
                    &format!("Response from {ip} for `{upload}`: `{text}`",),
                )
                .await;
            } else {
                msgs::warn(
                    msg_tx,
                    module,
                    &format!(
                        "Failed to post upload to {ip} `{upload}`: HTTP {}",
                        response.status()
                    ),
                )
                .await;
            }
        }
        Err(e) => {
            msgs::warn(
                msg_tx,
                module,
                &format!("Error posting upload to {ip} `{upload}`: {e}"),
            )
            .await;
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct LogData {
    pub name: String,
    pub ts: u64,
    pub plugin: String,
    pub level: String,
    pub msg: String,
}

impl fmt::Display for LogData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:<12} {} {:>10}: [{}] {}",
            self.name,
            utils::time::ts_str(self.ts),
            self.plugin,
            common::level_str(&self.level),
            self.msg
        )
    }
}

#[derive(Deserialize, Serialize)]
pub struct LogRequest {
    pub data: LogData,
}

impl fmt::Display for LogRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data.msg)
    }
}

// Note: do not pring anything else it will ping-pong the API
pub async fn post_log(ip: &str, log: &LogRequest) {
    let client = reqwest::Client::new();
    let _ = client
        .post(format!("http://{ip}:{}/{}", consts::WEB_PORT, Action::Log))
        .json(log)
        .send()
        .await;
}
