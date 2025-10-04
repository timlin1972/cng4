use std::fmt;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Msg};
use crate::utils::{self, common, nas};

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
pub struct UploadRequest {
    pub filename: String,
    pub content: String,
    pub mtime: String,
}

impl fmt::Display for UploadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", common::shorten(&self.filename, 20, 0))
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
                msgs::info(
                    msg_tx,
                    module,
                    &format!("Response from {ip} for `{upload}`: Ok",),
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

pub async fn upload_file(
    msg_tx: &Sender<Msg>,
    module: &str,
    ip: &str,
    source_path: &str,
    filename: &str,
) {
    let source_path = PathBuf::from(source_path);

    let bytes = fs::read(&source_path)
        .unwrap_or_else(|_| panic!("Failed to read file {}", source_path.display()));

    let encoded = nas::encode(&bytes);
    let mtime = fs::metadata(&source_path)
        .and_then(|meta| meta.modified())
        .map(|time| DateTime::<Utc>::from(time).to_rfc3339())
        .unwrap_or_else(|_| Utc::now().to_rfc3339());

    post_upload(
        msg_tx,
        module,
        ip,
        &UploadRequest {
            filename: filename.to_string(),
            content: encoded,
            mtime,
        },
    )
    .await;
}

#[derive(Deserialize, Serialize)]
pub struct DownloadData {
    pub filename: String,
}

#[derive(Deserialize, Serialize)]
pub struct DownloadRequest {
    pub data: DownloadData,
}

impl fmt::Display for DownloadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", common::shorten(&self.data.filename, 20, 0))
    }
}

#[derive(Deserialize, Serialize)]
pub struct DownloadResponseData {
    pub filename: String,
    pub content: String,
    pub mtime: String,
}

#[derive(Deserialize, Serialize)]
pub struct DownloadResponse {
    pub data: DownloadResponseData,
}

impl fmt::Display for DownloadResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", common::shorten(&self.data.filename, 20, 0))
    }
}

pub async fn post_download(
    msg_tx: &Sender<Msg>,
    module: &str,
    ip: &str,
    download: &DownloadRequest,
) -> anyhow::Result<DownloadResponse> {
    msgs::info(
        msg_tx,
        module,
        &format!("POST /download to {ip} `{download}`"),
    )
    .await;

    let client = reqwest::Client::new();
    let ret = client
        .post(format!("http://{ip}:{}/download", consts::WEB_PORT,))
        .json(download)
        .send()
        .await;

    match ret {
        Ok(response) => {
            if response.status().is_success() {
                let text = response.text().await.unwrap_or_default();
                msgs::info(
                    msg_tx,
                    module,
                    &format!("Response from {ip} for `{download}`: Ok",),
                )
                .await;
                let download_response: DownloadResponse = match serde_json::from_str(&text) {
                    Ok(dr) => dr,
                    Err(e) => return Err(anyhow::anyhow!(e.to_string())),
                };
                Ok(download_response)
            } else {
                msgs::warn(
                    msg_tx,
                    module,
                    &format!(
                        "Failed to post download to {ip} `{download}`: HTTP {}",
                        response.status()
                    ),
                )
                .await;
                Err(anyhow::anyhow!(format!(
                    "Failed to post download to {ip} `{download}`: HTTP {}",
                    response.status()
                )))
            }
        }
        Err(e) => {
            msgs::warn(
                msg_tx,
                module,
                &format!("Error posting download to {ip} `{download}`: {e}"),
            )
            .await;
            Err(anyhow::anyhow!(format!(
                "Error posting download to {ip} `{download}`: {e}"
            )))
        }
    }
}

pub async fn download_file(msg_tx: &Sender<Msg>, module: &str, ip: &str, remote_path: &str) {
    match post_download(
        msg_tx,
        module,
        ip,
        &DownloadRequest {
            data: DownloadData {
                filename: remote_path.to_string(),
            },
        },
    )
    .await
    {
        Ok(response) => {
            if let Err(e) = nas::write_file(
                &response.data.filename,
                &response.data.content,
                &response.data.mtime,
            )
            .await
            {
                msgs::warn(
                    msg_tx,
                    module,
                    &format!(
                        "Failed to write downloaded file `{}`: {e}",
                        response.data.filename
                    ),
                )
                .await;
            } else {
                msgs::info(
                    msg_tx,
                    module,
                    &format!("Downloaded file `{}` saved", response.data.filename),
                )
                .await;
            }
        }
        Err(e) => {
            msgs::warn(
                msg_tx,
                module,
                &format!("Failed to download file `{remote_path}`: {e}"),
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

#[derive(Deserialize, Serialize)]
pub struct GetFolderMetaRequest {
    pub foldername: String,
}

impl fmt::Display for GetFolderMetaRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.foldername)
    }
}

pub async fn post_get_folder_meta(
    msg_tx: &Sender<Msg>,
    module: &str,
    ip: &str,
    folder_meta: &GetFolderMetaRequest,
) -> anyhow::Result<nas::FolderMeta> {
    let client = reqwest::Client::new();
    let ret = client
        .post(format!("http://{ip}:{}/get/folder_meta", consts::WEB_PORT,))
        .json(folder_meta)
        .send()
        .await;

    match ret {
        Ok(response) => {
            if response.status().is_success() {
                let text = response.text().await.unwrap_or_default();
                msgs::info(
                    msg_tx,
                    module,
                    &format!(
                        "<- POST {ip} /get/folder_meta: `{}`: Ok",
                        folder_meta.foldername
                    ),
                )
                .await;
                let folder_meta: nas::FolderMeta = match serde_json::from_str(&text) {
                    Ok(fm) => fm,
                    Err(e) => return Err(anyhow::anyhow!(e.to_string())),
                };
                Ok(folder_meta)
            } else {
                Err(anyhow::anyhow!(format!(
                    "<- POST {ip} /get/folder_meta: `{}`: Failed, HTTP {}",
                    folder_meta.foldername,
                    response.status()
                )))
            }
        }
        Err(e) => Err(anyhow::anyhow!(format!(
            "<- POST {ip} /get/folder_meta: `{}`: Failed: {e}",
            folder_meta.foldername
        ))),
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RemoveRequest {
    pub filename: String,
}

impl fmt::Display for RemoveRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.filename)
    }
}

pub async fn post_remove(msg_tx: &Sender<Msg>, module: &str, ip: &str, remove_req: &RemoveRequest) {
    msgs::info(
        msg_tx,
        module,
        &format!("-> `{ip}`: remove `{}`", remove_req.filename),
    )
    .await;

    let client = reqwest::Client::new();
    let ret = client
        .post(format!(
            "http://{ip}:{}/{}",
            consts::WEB_PORT,
            Action::Remove
        ))
        .json(remove_req)
        .send()
        .await;

    match ret {
        Ok(response) => {
            if response.status().is_success() {
                msgs::info(
                    msg_tx,
                    module,
                    &format!("<- `{ip}`: remove `{}`: Ok", remove_req.filename),
                )
                .await;
            } else {
                msgs::warn(
                    msg_tx,
                    module,
                    &format!(
                        "Failed to post remove to `{ip}`: remove `{}`: HTTP {}",
                        remove_req.filename,
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
                &format!(
                    "Error posting remove to `{ip}`: remove `{}`: {e}",
                    remove_req.filename
                ),
            )
            .await;
        }
    }
}
