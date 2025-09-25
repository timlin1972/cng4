mod arguments;

use arguments::Arguments;
use clap::Parser;

fn main() {
    let args = Arguments::parse();

    println!("Args: {args:?}");
}
