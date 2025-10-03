use std::io::Write;

use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::Sender;
use tokio::time::Duration;

use crate::consts;
use crate::messages::{self as msgs, Action, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::{common, time};

pub const MODULE: &str = "cli";
const STARTUP_DELAY_SECS: u64 = 3;

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx: msg_tx.clone(),
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;
        tokio::spawn(start_input_loop(self.msg_tx.clone()));
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

async fn start_input_loop(msg_tx: Sender<Msg>) {
    msgs::info(
        &msg_tx,
        MODULE,
        &format!("Waiting for {STARTUP_DELAY_SECS} seconds before starting CLI input loop...",),
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
        }
    }
}
