use std::fmt;

use log::Level::{Error, Info, Warn};
use strum_macros::{AsRefStr, Display, EnumString};
use tokio::sync::{
    broadcast,
    mpsc::{Receiver, Sender},
};

use crate::consts;
use crate::plugins::{plugin_log, plugins_main};
use crate::utils::time;

const MODULE: &str = "messages";

// for weather
#[derive(EnumString, AsRefStr, Display, PartialEq, Clone, Debug)]
pub enum WeatherKey {
    #[strum(serialize = "summary")]
    Summary,
    #[strum(serialize = "daily")]
    Daily,
}

// for infos
#[derive(EnumString, AsRefStr, Display, PartialEq, Clone, Debug)]
pub enum InfoKey {
    #[strum(serialize = "devices")]
    Devices,
    #[strum(serialize = "weather")]
    Weather,
}

// for devices
#[derive(EnumString, AsRefStr, Display, PartialEq, Clone, Debug)]
pub enum DeviceKey {
    #[strum(serialize = "onboard")]
    Onboard,
    #[strum(serialize = "version")]
    Version,
    #[strum(serialize = "tailscale_ip")]
    TailscaleIp,
    #[strum(serialize = "temperature")]
    Temperature,
    #[strum(serialize = "app_uptime")]
    AppUptime,
}

// for Key
#[derive(EnumString, AsRefStr, Display, PartialEq, Clone, Debug)]
pub enum Key {
    #[strum(serialize = "tab")]
    Tab,
    #[strum(serialize = "up")]
    Up,
    #[strum(serialize = "down")]
    Down,
    #[strum(serialize = "left")]
    Left,
    #[strum(serialize = "right")]
    Right,
    #[strum(serialize = "alt_c")]
    AltC,
    #[strum(serialize = "alt_up")]
    AltUp,
    #[strum(serialize = "alt_down")]
    AltDown,
    #[strum(serialize = "alt_left")]
    AltLeft,
    #[strum(serialize = "alt_right")]
    AltRight,
    #[strum(serialize = "alt_w")]
    AltW,
    #[strum(serialize = "alt_s")]
    AltS,
    #[strum(serialize = "alt_a")]
    AltA,
    #[strum(serialize = "alt_d")]
    AltD,
    #[strum(serialize = "ctrl_x")]
    ControlX,
    #[strum(serialize = "ctrl_s")]
    ControlS,
    #[strum(serialize = "home")]
    Home,
    #[strum(serialize = "end")]
    End,
}

#[derive(EnumString, AsRefStr, Display, PartialEq, Clone, Debug)]
pub enum Action {
    #[strum(serialize = "log")]
    Log,
    #[strum(serialize = "insert")]
    Insert,
    #[strum(serialize = "show")]
    Show,
    #[strum(serialize = "update")]
    Update,
    #[strum(serialize = "download")]
    Download,
    #[strum(serialize = "help")]
    Help,
    #[strum(serialize = "gui")]
    Gui,
    #[strum(serialize = "output_update")]
    OutputUpdate,
    #[strum(serialize = "output_push")]
    OutputPush,
    #[strum(serialize = "key")]
    Key,
    #[strum(serialize = "restart")]
    Restart,
    #[strum(serialize = "disconnected")]
    Disconnected,
    #[strum(serialize = "publish")]
    Publish,
    #[strum(serialize = "add")]
    Add,
    #[strum(serialize = "cmd")]
    Cmd,
    #[strum(serialize = "upload")]
    Upload,
    #[strum(serialize = "remove")]
    Remove,
    #[strum(serialize = "dest")]
    Dest,
    #[strum(serialize = "open")]
    Open,
    #[strum(serialize = "popup")]
    Popup,
    #[strum(serialize = "insert_panel")]
    InsertPanel,
    #[strum(serialize = "redraw")]
    Redraw,
    #[strum(serialize = "sync")]
    Sync,
}

#[derive(Debug, Clone)]
pub struct Cmd {
    pub cmd: String,
}

impl fmt::Display for Cmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[CMD] {}", self.cmd)
    }
}

#[derive(Debug, Clone)]
pub enum Data {
    Cmd(Cmd),
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Data::Cmd(cmd) => write!(f, "Cmd: {cmd}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Msg {
    pub ts: u64,
    pub plugin: String,
    pub data: Data,
}

impl Msg {
    pub fn new(plugin: &str, data: Data) -> Self {
        Self {
            ts: time::ts(),
            plugin: plugin.to_string(),
            data,
        }
    }
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "time: {}, plugin: {}, data: {}",
            time::ts_str(self.ts),
            self.plugin,
            self.data
        )
    }
}

pub struct Messages {
    msg_tx: Sender<Msg>,
}

impl Messages {
    pub async fn new(
        msg_tx: Sender<Msg>,
        shutdown_tx: broadcast::Sender<()>,
        mut msg_rx: Receiver<Msg>,
        mut plugins: plugins_main::Plugins,
    ) -> Self {
        let myself = Self {
            msg_tx: msg_tx.clone(),
        };
        myself.info(consts::NEW.to_string()).await;

        tokio::spawn(async move {
            info(&msg_tx, MODULE, "  Starting to receive messages...").await;
            while let Some(msg) = msg_rx.recv().await {
                handle_msg(&msg, &msg_tx, &mut plugins, &shutdown_tx).await;
            }
        });

        myself
    }

    async fn info(&self, msg: String) {
        info(&self.msg_tx, MODULE, &msg).await;
    }
}

async fn handle_msg(
    msg: &Msg,
    msg_tx: &Sender<Msg>,
    plugins: &mut plugins_main::Plugins,
    shutdown_tx: &broadcast::Sender<()>,
) {
    match msg.data {
        Data::Cmd(_) => handle_msg_cmd(msg, msg_tx, plugins, shutdown_tx).await,
    }
}

async fn handle_msg_cmd(
    msg: &Msg,
    msg_tx: &Sender<Msg>,
    plugins: &mut plugins_main::Plugins,
    shutdown_tx: &broadcast::Sender<()>,
) {
    let Data::Cmd(cmd) = &msg.data;
    let cmd = &cmd.cmd;
    let cmd = cmd
        .split_once(consts::COMMENT)
        .map(|(before, _)| before.trim_end())
        .unwrap_or(cmd);
    let cmd_parts: Vec<&str> = cmd.split_whitespace().collect();
    if cmd_parts.is_empty() {
        info(msg_tx, MODULE, "").await;
        return;
    }

    let command = cmd_parts[0];
    match command {
        consts::P => plugins.handle_cmd(msg).await,
        consts::Q | consts::QUIT | consts::EXIT => {
            let _ = shutdown_tx.send(());
        }
        _ => warn(msg_tx, MODULE, &format!("Unknown command: `{command}`")).await,
    }
}

//
// Helper functions to send messages
//

pub async fn cmd(msg_tx: &Sender<Msg>, module: &str, cmd: &str) {
    let _ = msg_tx
        .send(Msg::new(
            module,
            Data::Cmd(Cmd {
                cmd: cmd.to_string(),
            }),
        ))
        .await;
}

async fn log(msg_tx: &Sender<Msg>, level: log::Level, module: &str, msg: &str) {
    let msg = msg.replace("'", "_");
    cmd(
        msg_tx,
        module,
        &format!(
            "{} {} {} {level} '{msg}'",
            consts::P,
            plugin_log::MODULE,
            Action::Log,
        ),
    )
    .await;
}

pub async fn info(msg_tx: &Sender<Msg>, module: &str, msg: &str) {
    log(msg_tx, Info, module, msg).await;
}

pub async fn warn(msg_tx: &Sender<Msg>, module: &str, msg: &str) {
    log(msg_tx, Warn, module, msg).await;
}

pub async fn error(msg_tx: &Sender<Msg>, module: &str, msg: &str) {
    log(msg_tx, Error, module, msg).await;
}
