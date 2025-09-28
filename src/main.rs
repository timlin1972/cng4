use std::env;

use clap::Parser;
use tokio::sync::{broadcast, mpsc};

mod arguments;
mod consts;
mod handle_panic;
mod messages;
mod plugins;
mod utils;

use arguments::Arguments;
use messages::{self as msgs, Messages, Msg};
use plugins::{plugin_cfg, plugin_log, plugins_main::Plugins};
use utils::common;

const MODULE: &str = "main";

async fn print_startup_message(msg_tx: &mpsc::Sender<Msg>) {
    msgs::info(
        msg_tx,
        MODULE,
        &format!(
            "Starting {} v{}...",
            common::get_binary_name(),
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
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);

    // log startup message
    print_startup_message(&msg_tx).await;

    // handle args
    let args = Arguments::parse();
    msgs::info(&msg_tx, MODULE, &format!("{args}")).await;

    // plugins
    let mut plugins =
        Plugins::new(msg_tx.clone(), shutdown_tx.clone(), args.mode, &args.script).await;

    // insert minimum set of plugins
    let _ = plugins.insert(plugin_log::MODULE).await;
    let _ = plugins.insert(plugin_cfg::MODULE).await;

    // messages
    let _ = Messages::new(msg_tx.clone(), shutdown_tx.clone(), msg_rx, plugins).await;

    // wait for 10 seconds then send shutdown signal again to force exit
    let _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    let _ = shutdown_tx.send(());

    // wait for shutdown signal
    let _ = tokio::spawn(async move { if shutdown_rx.recv().await.is_ok() {} }).await;
}
