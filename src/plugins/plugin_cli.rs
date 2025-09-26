use std::io::Write;

use async_trait::async_trait;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::Sender;
use tokio::time::Duration;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg};
use crate::plugins::plugins_main;
use crate::utils::{common, time};

pub const MODULE: &str = "cli";
const STARTUP_DELAY_SECS: u64 = 3;

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>) -> Self {
        let myself = Self {
            msg_tx: msg_tx.clone(),
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        myself
    }

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;
        tokio::spawn(start_input_loop_cli(self.msg_tx.clone()));
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
    }
}

#[async_trait]
impl plugins_main::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let (_cmd_parts, action) = match common::get_cmd_action(&cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Show => self.handle_cmd_show().await,
            _ => {
                self.warn(format!("[{MODULE}] Unsupported action: {action}"))
                    .await
            }
        }
    }
}

//
// CLI input handling
//

fn prompt() {
    print!("{} > ", time::ts_str(time::ts()));
    std::io::stdout()
        .flush()
        .map_err(|e| e.to_string())
        .expect("Failed to flush");
}

async fn start_input_loop_cli(msg_tx: Sender<Msg>) {
    msgs::info(
        &msg_tx,
        MODULE,
        &format!(
            "Waiting for {} seconds before starting CLI input loop...",
            STARTUP_DELAY_SECS
        ),
    )
    .await;
    tokio::time::sleep(Duration::from_secs(STARTUP_DELAY_SECS)).await;

    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    loop {
        prompt();
        tokio::select! {
                maybe_line = lines.next_line() => {
                    match maybe_line {
                        Ok(Some(line)) => {
                            msgs::cmd(&msg_tx, MODULE, &line).await;
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        Ok(None) => break, // EOF
                        Err(e) => {
                            msgs::warn(&msg_tx, MODULE, &format!("Failed to read input. Err: {e}")).await;
                            break;
                        }
                    }
                }
        //         _ = shutdown_rx.recv() => {
        //             break;
        //         }
            }
    }
}
