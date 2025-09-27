use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::common;

pub const MODULE: &str = "web";

async fn msgs_info(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::info(msg_tx, MODULE, msg).await;
}

async fn msgs_cmd(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::cmd(msg_tx, MODULE, msg).await;
}

#[get("/hello")]
async fn hello(msg_tx: web::Data<Sender<Msg>>) -> impl Responder {
    msgs_info(&msg_tx, "API: GET /hello").await;
    HttpResponse::Ok().body(format!("Hello {}!", common::get_binary_name()))
}

#[derive(Deserialize)]
struct CmdRequest {
    cmd: String,
}

#[post("/cmd")]
async fn cmd(data: web::Json<CmdRequest>, msg_tx: web::Data<Sender<Msg>>) -> impl Responder {
    let data_cmd = &data.cmd;
    msgs_info(&msg_tx, &format!("API: POST /cmd: `{data_cmd}`")).await;
    msgs_cmd(&msg_tx, data_cmd).await;

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
                    .service(hello)
                    .service(cmd)
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
                self.warn(format!("[{MODULE}] Unsupported action: {action}"))
                    .await
            }
        }
    }
}
