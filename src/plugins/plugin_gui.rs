use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::{Mutex, broadcast, mpsc::Sender};
use tokio::task;

use crate::consts;
use crate::messages::{self as msgs, Action, Key, Msg};
use crate::plugins::plugins_main::{self, Plugin};
use crate::utils::common;

pub const MODULE: &str = "gui";
const PROMPT: &str = "> ";
const OUTPUT_PANEL: &str = "command";

#[derive(Debug)]
pub struct PluginUnit {
    msg_tx: Sender<Msg>,
    shutdown_tx: broadcast::Sender<()>,
    output: Arc<Mutex<String>>,
    history: Arc<Mutex<Vec<String>>>,
    history_index: Arc<Mutex<usize>>,
}

impl PluginUnit {
    pub async fn new(msg_tx: Sender<Msg>, shutdown_tx: broadcast::Sender<()>) -> Result<Self> {
        let myself = Self {
            msg_tx: msg_tx.clone(),
            shutdown_tx,
            output: Arc::new(Mutex::new(String::new())),
            history: Arc::new(Mutex::new(vec![])),
            history_index: Arc::new(Mutex::new(0)),
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;

        // update prompt
        output_update(&self.msg_tx, PROMPT).await;

        let shutdown_rx = self.shutdown_tx.subscribe();
        let output_clone = Arc::clone(&self.output);
        let history_clone = Arc::clone(&self.history);
        let history_index_clone = Arc::clone(&self.history_index);
        tokio::spawn(start_input_loop(
            self.msg_tx.clone(),
            shutdown_rx,
            output_clone,
            history_clone,
            history_index_clone,
        ));
    }

    async fn handle_action_show(&self) {
        self.info(Action::Show.to_string()).await;
    }

    async fn handle_action_help(&self) {
        self.info(Action::Help.to_string()).await;
    }

    async fn handle_action_key_up(&mut self) {
        let mut output = self.output.lock().await;
        let history = self.history.lock().await;
        let mut history_index = self.history_index.lock().await;

        if *history_index > 0 {
            *history_index -= 1;
            *output = history[*history_index].clone();
        }

        output_update(&self.msg_tx, &format!("{PROMPT}{output}")).await;
    }

    async fn handle_action_key_down(&mut self) {
        let mut output = self.output.lock().await;
        let history = self.history.lock().await;
        let mut history_index = self.history_index.lock().await;

        if *history_index < history.len() {
            *history_index += 1;
            if *history_index < history.len() {
                *output = history[*history_index].clone();
            } else {
                output.clear();
            }
        }

        output_update(&self.msg_tx, &format!("{PROMPT}{output}")).await;
    }

    async fn handle_action_key(&mut self, cmd_parts: &[String]) {
        if let Some(key) = cmd_parts.get(3) {
            match key.parse::<Key>() {
                Ok(Key::Up) => self.handle_action_key_up().await,
                Ok(Key::Down) => self.handle_action_key_down().await,
                _ => (),
            }
        }
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

    async fn handle_action(&mut self, action: Action, cmd_parts: &[String], _msg: &Msg) {
        match action {
            Action::Help => self.handle_action_help().await,
            Action::Show => self.handle_action_show().await,
            Action::Key => self.handle_action_key(cmd_parts).await,
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

async fn handle_keycode_char(output: &Arc<Mutex<String>>, msg_tx: &Sender<Msg>, key: char) {
    let mut output = output.lock().await;
    output.push(key);
    output_update(msg_tx, &format!("{PROMPT}{output}")).await;
}

async fn handle_keycode_backspace(output: &Arc<Mutex<String>>, msg_tx: &Sender<Msg>) {
    let mut output = output.lock().await;
    output.pop();
    output_update(msg_tx, &format!("{PROMPT}{output}")).await;
}

async fn handle_keycode_enter(
    output: &Arc<Mutex<String>>,
    msg_tx: &Sender<Msg>,
    history: &Arc<Mutex<Vec<String>>>,
    history_index: &Arc<Mutex<usize>>,
) {
    let mut output = output.lock().await;
    let mut history = history.lock().await;
    let mut history_index = history_index.lock().await;

    // ignore if the input is as the same as the last one
    if history.is_empty() || *history.last().unwrap() != *output {
        // ignore enter only
        if !output.is_empty() {
            history.push(output.clone());
            *history_index = history.len();
        }
    }

    msgs::cmd(msg_tx, MODULE, &output).await;

    output.clear();
    output_update(msg_tx, &format!("{PROMPT}{output}")).await;
}

async fn handle_keycode_key(msg_tx: &Sender<Msg>, key: Key) {
    msgs::cmd(
        msg_tx,
        MODULE,
        &format!(
            "{} {} {} {key}",
            consts::P,
            plugins_main::MODULE,
            Action::Key
        ),
    )
    .await;
}

async fn handle_keycode(
    output: &Arc<Mutex<String>>,
    msg_tx: &Sender<Msg>,
    key: KeyCode,
    history: &Arc<Mutex<Vec<String>>>,
    history_index: &Arc<Mutex<usize>>,
) {
    match key {
        // Normal character input
        KeyCode::Char(c) => handle_keycode_char(output, msg_tx, c).await,
        KeyCode::Backspace => handle_keycode_backspace(output, msg_tx).await,
        KeyCode::Enter => handle_keycode_enter(output, msg_tx, history, history_index).await,

        // Special keys (send to panels plugin)
        KeyCode::Tab => handle_keycode_key(msg_tx, Key::Tab).await,
        KeyCode::Up => handle_keycode_key(msg_tx, Key::Up).await,
        KeyCode::Down => handle_keycode_key(msg_tx, Key::Down).await,
        KeyCode::Left => handle_keycode_key(msg_tx, Key::Left).await,
        KeyCode::Right => handle_keycode_key(msg_tx, Key::Right).await,
        _ => {}
    }
}

async fn handle_keycode_alt(msg_tx: &Sender<Msg>, key: KeyCode) {
    match key {
        KeyCode::Char('c') => handle_keycode_key(msg_tx, Key::AltC).await,
        KeyCode::Up => handle_keycode_key(msg_tx, Key::AltUp).await,
        KeyCode::Down => handle_keycode_key(msg_tx, Key::AltDown).await,
        KeyCode::Left => handle_keycode_key(msg_tx, Key::AltLeft).await,
        KeyCode::Right => handle_keycode_key(msg_tx, Key::AltRight).await,
        KeyCode::Char('w') => handle_keycode_key(msg_tx, Key::AltW).await,
        KeyCode::Char('s') => handle_keycode_key(msg_tx, Key::AltS).await,
        KeyCode::Char('a') => handle_keycode_key(msg_tx, Key::AltA).await,
        KeyCode::Char('d') => handle_keycode_key(msg_tx, Key::AltD).await,
        _ => (),
    };
}

async fn handle_keycode_control(msg_tx: &Sender<Msg>, key: KeyCode) {
    match key {
        KeyCode::Char('x') => handle_keycode_key(msg_tx, Key::ControlX).await,
        KeyCode::Char('s') => handle_keycode_key(msg_tx, Key::ControlS).await,
        _ => (),
    };
}

async fn start_input_loop(
    msg_tx: Sender<Msg>,
    mut shutdown_rx: broadcast::Receiver<()>,
    output: Arc<Mutex<String>>,
    history: Arc<Mutex<Vec<String>>>,
    history_index: Arc<Mutex<usize>>,
) {
    // 建立 channel 傳送 key event（spawn_blocking 到 async）
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<KeyEvent>(32);
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    let shutdown_flag_clone = shutdown_flag.clone();

    let input_task = task::spawn_blocking(move || {
        loop {
            // 非同步 poll，避免卡住
            if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
                #[allow(clippy::collapsible_if)]
                if let Ok(Event::Key(key)) = event::read() {
                    // 把 key 傳出去給 async task 處理
                    if input_tx.blocking_send(key).is_err() {
                        break;
                    }
                }
            }

            // 用 channel 檢查是否該退出（後面 async 部分會處理這個）
            if shutdown_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
        }
    });

    loop {
        tokio::select! {
            Some(key) = input_rx.recv() => {
                if key.modifiers == KeyModifiers::ALT {
                    handle_keycode_alt(&msg_tx, key.code).await;
                } else if key.modifiers == KeyModifiers::CONTROL {
                    handle_keycode_control(&msg_tx, key.code).await;
                } else {
                    handle_keycode(&output, &msg_tx, key.code, &history, &history_index).await;
                }
            }
            _ = shutdown_rx.recv() => {
                shutdown_flag_clone.store(true, Ordering::Relaxed);
                break;
            }
        }
    }

    // 等待 blocking thread 結束
    let _ = input_task.await;
}

async fn output_update(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::cmd(
        msg_tx,
        MODULE,
        &format!(
            "{} {OUTPUT_PANEL} {} '{msg}'",
            consts::P,
            Action::OutputUpdate,
        ),
    )
    .await;
}
