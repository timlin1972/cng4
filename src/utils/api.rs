use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Msg};

#[derive(Deserialize, Serialize, Debug)]
pub struct CmdRequest {
    pub cmd: String,
}

impl fmt::Display for CmdRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cmd)
    }
}

pub async fn post_cmd(msg_tx: &Sender<Msg>, module: &str, ip: &str, cmd: &CmdRequest) {
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
                    &format!("Response from {ip} for `{cmd}`: `{text}`"),
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
        write!(f, "{}", self.data.filename)
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
