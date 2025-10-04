use std::fs;
use std::path::{Path, PathBuf};

use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::{api, common, nas};

pub const MODULE: &str = "web";
const MAX_SIZE: usize = 100 * 1024 * 1024; // 100MB

fn is_valid_filename(path: &str) -> bool {
    let path = Path::new(path);
    path.components().all(|c| {
        matches!(
            c,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) && !path.is_absolute()
}

async fn msgs_info(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::info(msg_tx, MODULE, msg).await;
}

async fn msgs_warn(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::warn(msg_tx, MODULE, msg).await;
}

async fn msgs_cmd(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::cmd(msg_tx, MODULE, msg).await;
}

#[get("/hello")]
async fn hello(msg_tx: web::Data<Sender<Msg>>) -> impl Responder {
    msgs_info(&msg_tx, "API: GET /hello").await;
    HttpResponse::Ok().body(format!("Hello {}!", common::get_binary_name()))
}

#[post("/cmd")]
async fn cmd(data: web::Json<api::CmdRequest>, msg_tx: web::Data<Sender<Msg>>) -> impl Responder {
    let data_cmd = &data.cmd;

    msgs_info(&msg_tx, &format!("API: POST /cmd: `{data_cmd}`")).await;
    msgs_cmd(&msg_tx, data_cmd).await;

    HttpResponse::Ok().finish()
}

#[post("/upload")]
async fn upload(
    data: web::Json<api::UploadRequest>,
    msg_tx: web::Data<Sender<Msg>>,
) -> impl Responder {
    let filename = &data.filename;

    msgs_info(&msg_tx, &format!("API: POST /upload: `{filename}`")).await;

    if !is_valid_filename(filename) {
        return HttpResponse::BadRequest().body("Invalid filename");
    }

    let content = &data.content;
    let mtime = &data.mtime;

    if let Err(e) = nas::write_file(filename, content, mtime).await {
        msgs_warn(&msg_tx, &format!("Failed to write `{filename}`: {e}")).await;
        return HttpResponse::InternalServerError().body("Failed to write `{filename}`: {e}");
    }

    msgs_info(&msg_tx, &format!("API: POST /upload: `{filename}` done")).await;

    HttpResponse::Ok().finish()
}

#[post("/download")]
async fn download(
    data: web::Json<api::DownloadRequest>,
    msg_tx: web::Data<Sender<Msg>>,
) -> impl Responder {
    let filename = &data.data.filename;

    msgs_info(&msg_tx, &format!("API: POST /download: `{filename}`")).await;

    if !is_valid_filename(filename) {
        return HttpResponse::BadRequest().body("Invalid filename");
    }

    let path = PathBuf::from(filename);

    match fs::read(&path) {
        Ok(bytes) => {
            let metadata_modified = fs::metadata(&path).unwrap().modified().unwrap();

            let mtime = nas::mtime_str(metadata_modified);

            let encoded = nas::encode(&bytes);

            msgs_info(&msg_tx, &format!("API: POST /download: `{filename}` done")).await;

            HttpResponse::Ok().json(api::DownloadResponse {
                data: api::DownloadResponseData {
                    filename: filename.clone(),
                    content: encoded,
                    mtime,
                },
            })
        }
        Err(_) => HttpResponse::NotFound().json(json!({
            "error": "Not Found",
            "message": "指定的資源不存在"
        })),
    }
}

#[post("/remove")]
async fn remove(
    data: web::Json<api::RemoveRequest>,
    msg_tx: web::Data<Sender<Msg>>,
) -> impl Responder {
    let filename = &data.filename;

    msgs_info(&msg_tx, &format!("API: POST /remove: `{filename}`")).await;

    if !is_valid_filename(filename) {
        return HttpResponse::BadRequest().body("Invalid filename");
    }

    match fs::remove_file(filename) {
        Ok(_) => {
            msgs_info(&msg_tx, &format!("API: POST /remove: `{filename}` done")).await;
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            msgs_warn(&msg_tx, &format!("Failed to remove `{filename}`: {e}")).await;
            HttpResponse::InternalServerError().body(format!("Failed to remove `{filename}`: {e}"))
        }
    }
}

#[post("/log")]
async fn log(data: web::Json<api::LogRequest>, msg_tx: web::Data<Sender<Msg>>) -> impl Responder {
    let data_log = &data.data;
    msgs_info(&msg_tx, &format!("{data_log}")).await;

    HttpResponse::Ok().finish()
}

#[post("/get/folder_meta")]
async fn get_folder_meta(
    data: web::Json<api::GetFolderMetaRequest>,
    msg_tx: web::Data<Sender<Msg>>,
) -> impl Responder {
    let foldername = &data.foldername;
    msgs_info(
        &msg_tx,
        &format!("API: POST /get/folder_meta: `{foldername}`"),
    )
    .await;

    let folder_meta = nas::get_folder_meta(foldername);

    msgs_info(
        &msg_tx,
        &format!("API: POST /get/folder_meta: `{foldername}` done"),
    )
    .await;

    HttpResponse::Ok().json(folder_meta)
}

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let mut myself = Self { msg_tx };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn init(&mut self) {
        self.info(consts::INIT.to_string()).await;

        let _ = std::fs::create_dir_all(consts::NAS_UPLOAD_FOLDER);

        self.info(format!(
            "  Running web server on {}:{}...",
            consts::WEB_IP,
            consts::WEB_PORT
        ))
        .await;

        let msg_tx_clone = self.msg_tx.clone();

        tokio::spawn(async move {
            HttpServer::new(move || {
                App::new()
                    .app_data(web::Data::new(msg_tx_clone.clone()))
                    .app_data(web::JsonConfig::default().limit(MAX_SIZE)) // 100 MB
                    .service(hello)
                    .service(cmd)
                    .service(upload)
                    .service(download)
                    .service(remove)
                    .service(log)
                    .service(get_folder_meta)
            })
            .bind((consts::WEB_IP, consts::WEB_PORT))
            .unwrap()
            .run()
            .await
        });
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
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

    async fn handle_action(&mut self, action: Action, _cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
