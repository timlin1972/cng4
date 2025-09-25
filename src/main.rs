mod arguments;
mod handle_panic;
mod messages;
mod utils;

use arguments::Arguments;
use clap::Parser;

const MODULE: &str = "main";

#[actix_web::main]
async fn main() {
    handle_panic::handle_panic();
    let args = Arguments::parse();
    println!("Arguments: {:?}", args);

    let messages = messages::Messages::new().await;

    let msg = messages::Msg::new(MODULE);
    let _ = messages.msg_tx.send(msg).await;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
