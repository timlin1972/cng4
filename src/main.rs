mod arguments;
mod handle_panic;

use arguments::Arguments;
use clap::Parser;

fn main() {
    handle_panic::handle_panic();
    let args = Arguments::parse();

    println!("Args: {args:?}");
}
