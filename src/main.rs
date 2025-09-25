use std::env;
use std::path::Path;

use clap::Parser;
use tokio::sync::mpsc;

mod arguments;
mod consts;
mod handle_panic;
mod messages;
mod plugins;
mod utils;

use arguments::Arguments;
use messages::{self as msgs, Messages, Msg};
use plugins::{plugin_cfg, plugin_log, plugins_main::Plugins};

const MODULE: &str = "main";

fn get_binary_name() -> String {
    if let Ok(path) = env::current_exe()
        && let Some(name) = path.file_name()
    {
        return name.to_string_lossy().into_owned();
    }

    panic!(
        "Failed to get binary name from path: {:?}",
        Path::new(&env::args().next().unwrap())
    );
}

async fn print_startup_message(msg_tx: &mpsc::Sender<Msg>) {
    msgs::info(
        msg_tx,
        MODULE,
        &format!(
            "Starting {} v{}...",
            get_binary_name(),
            env!("CARGO_PKG_VERSION")
        ),
    )
    .await;
}

#[actix_web::main]
async fn main() {
    // handle panic
    handle_panic::handle_panic();

    // channels
    let (msg_tx, msg_rx) = mpsc::channel::<Msg>(consts::MSG_SIZE);

    // log startup message
    print_startup_message(&msg_tx).await;

    // handle args
    let args = Arguments::parse();
    msgs::info(&msg_tx, MODULE, &format!("{args}")).await;

    // plugins
    let mut plugins = Plugins::new(msg_tx.clone(), args.mode, &args.script).await;
    plugins.insert(plugin_log::MODULE).await;
    plugins.insert(plugin_cfg::MODULE).await;

    // messages
    let _ = Messages::new(msg_tx.clone(), msg_rx, plugins).await;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
