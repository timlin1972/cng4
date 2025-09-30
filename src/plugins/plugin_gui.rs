use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::{Mutex, broadcast, mpsc::Sender};
use tokio::task;

use crate::consts;
use crate::globals;
use crate::messages::{self as msgs, Action, Data, Key, Msg};
use crate::plugins::{plugin_panels, plugins_main};
use crate::utils::{self, common};

pub const MODULE: &str = "gui";
const PROMPT: &str = "> ";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    shutdown_tx: broadcast::Sender<()>,
    output: Arc<Mutex<String>>,
    history: Arc<Mutex<Vec<String>>>,
    history_index: Arc<Mutex<usize>>,
}

impl Plugin {
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

        let shutdown_rx = self.shutdown_tx.subscribe();
        tokio::spawn(update_subtitle(self.msg_tx.clone(), shutdown_rx));
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

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
    }

    async fn handle_cmd_key_up(&mut self) {
        let mut output = self.output.lock().await;
        let history = self.history.lock().await;
        let mut history_index = self.history_index.lock().await;

        if *history_index > 0 {
            *history_index -= 1;
            *output = history[*history_index].clone();
        }

        output_update(&self.msg_tx, &format!("{PROMPT}{output}")).await;
    }

    async fn handle_cmd_key_down(&mut self) {
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

    async fn handle_cmd_key(&mut self, cmd_parts: &[String]) {
        if let Some(key) = cmd_parts.get(3) {
            match key.parse::<Key>() {
                Ok(Key::Up) => self.handle_cmd_key_up().await,
                Ok(Key::Down) => self.handle_cmd_key_down().await,
                _ => (),
            }
        }
    }
}

#[async_trait]
impl plugins_main::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let (cmd_parts, action) = match common::get_cmd_action(&cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            Action::Key => self.handle_cmd_key(&cmd_parts).await,
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
        history.push(output.clone());
        *history_index = history.len();
    }

    cmd(msg_tx, &output).await;

    output.clear();
    output_update(msg_tx, &format!("{PROMPT}{output}")).await;
}

async fn handle_keycode_key(msg_tx: &Sender<Msg>, key: Key) {
    cmd(
        msg_tx,
        &format!(
            "{} {} {} {}",
            consts::P,
            plugin_panels::MODULE,
            Action::Key,
            key
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
                }   else {
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
            "{} {} {} {} '{}'",
            consts::P,
            plugin_panels::MODULE,
            Action::OutputUpdate,
            consts::COMMAND,
            msg
        ),
    )
    .await;
}

async fn cmd(msg_tx: &Sender<Msg>, msg: &str) {
    msgs::cmd(msg_tx, MODULE, msg).await;
}

async fn update_subtitle(msg_tx: Sender<Msg>, mut shutdown_rx: broadcast::Receiver<()>) {
    let sys_name = globals::get_sys_name();
    let version = env!("CARGO_PKG_VERSION");
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                let ts = utils::time::ts();
                let sub_title = format!(" - {sys_name} (v{version}) - {}", utils::time::ts_str(ts));

                cmd(
                    &msg_tx,
                    &format!(
                        "{} {} {} {} '{sub_title}'",
                        consts::P,
                        plugin_panels::MODULE,
                        Action::SubTitle,
                        consts::COMMAND,
                    ),
                ).await;
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
}
