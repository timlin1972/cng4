use std::fs;
use std::path::Path;
use std::path::PathBuf;

use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};
use anyhow::Result;
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose;
use chrono::{DateTime, Utc};
use filetime::FileTime;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::{api, common};

pub const MODULE: &str = "web";
const MAX_SIZE: usize = 100 * 1024 * 1024; // 100MB

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
    let filename = &data.data.filename;

    msgs_info(&msg_tx, &format!("API: POST /upload: `{filename}`")).await;

    fn is_valid_filename(path: &str) -> bool {
        let path = Path::new(path);
        path.components().all(|c| {
            matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        }) && !path.is_absolute()
    }

    if !is_valid_filename(filename) {
        return HttpResponse::BadRequest().body("Invalid filename");
    }

    let content = &data.data.content;
    let mtime = &data.data.mtime;

    if let Err(e) = write_file(filename, content, mtime).await {
        msgs_warn(&msg_tx, &format!("Failed to write `{filename}`: {e}")).await;
        return HttpResponse::InternalServerError().body("Failed to write `{filename}`: {e}");
    }

    msgs_info(&msg_tx, &format!("API: POST /upload: `{filename}` done")).await;

    HttpResponse::Ok().finish()
}

#[post("/log")]
async fn log(data: web::Json<api::LogRequest>, msg_tx: web::Data<Sender<Msg>>) -> impl Responder {
    let data_log = &data.data;
    msgs_info(&msg_tx, &format!("{data_log}")).await;

    HttpResponse::Ok().finish()
}

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let mut myself = Self { msg_tx };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
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
                    .service(log)
            })
            .bind((consts::WEB_IP, consts::WEB_PORT))
            .unwrap()
            .run()
            .await
        });
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
    }
}

#[async_trait]
impl plugins_main::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(data_cmd) = &msg.data;

        let (_cmd_parts, action) = match common::get_cmd_action(&data_cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}

async fn write_file(filename: &str, content: &str, mtime: &str) -> anyhow::Result<()> {
    let file_path = PathBuf::from(filename);

    // if the content is the same, return
    if file_path.exists() {
        let bytes = fs::read(&file_path)?;
        let encoded = general_purpose::STANDARD.encode(&bytes);
        if encoded == content {
            return Ok(());
        }
    }

    let decoded = general_purpose::STANDARD.decode(content)?;
    let mtime: DateTime<Utc> = DateTime::parse_from_rfc3339(mtime)?.with_timezone(&Utc);

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&file_path, decoded)?;

    let file_time = FileTime::from_unix_time(mtime.timestamp(), 0);
    filetime::set_file_mtime(&file_path, file_time)?;

    Ok(())
}
