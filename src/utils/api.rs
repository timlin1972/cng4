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
        write!(f, "cmd: `{}`", self.cmd)
    }
}


pub async fn send_cmd(msg_tx: &Sender<Msg>, module: &str, ip: &str, cmd: &CmdRequest) {
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
                        "Failed to send command to {ip} `{cmd}`: HTTP {}",
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
                &format!("Error sending command to {ip} `{cmd}`: {e}"),
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

// pub async fn send_upload(msg_tx: &Sender<Msg>, module: &str, ip: &str, filename: &str, content: &[u8], mtime: &str) {
//     let client = reqwest::Client::new();
//     let _ = client
//         .post(format!("http://{ip}:{}/{}", consts::WEB_PORT, Action::Upload))
//         .json(&json!({
//             "data": {
//                 "filename": filename,
//                 "content": encoded,
//                 "mtime": mtime,
//             }
//         }))
//         .send()
//         .await;

// }
