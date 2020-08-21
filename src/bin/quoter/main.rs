#[macro_use]
extern crate log;

use clap::{App, Arg};

use blockswap::logging;
use blockswap::quoter::{database, vault_node, Quoter};
use std::sync::{Arc, Mutex};

/*
Entry point for the Quoter binary. We should try to keep it as small as posible
and implement most of the core logic as part of the library (src/lib.rs). This way
of organising code works better with integration tests.
 Ideally we would just parse commad line arguments here and call into the library.
*/

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|msg| {
        error!("Panicked with: {}", msg);
        std::process::exit(101); // Rust's panics use 101 by default
    }));

    let matches = App::new("Blockswap Quoter")
        .version("0.1")
        .about("A web server that provides swap quotes")
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true)
                .help("Port on which to listen for incoming connections"),
        )
        .get_matches();

    logging::init("quoter", None);

    let port = matches.value_of("port").unwrap_or("3033");

    if let Ok(port) = port.parse::<u16>() {
        info!("Starting the Blockswap Quoter");

        let database = database::Database::open("quoter.db");
        let database = Arc::new(Mutex::new(database));

        let vault_node_api = vault_node::VaultNodeAPI::new(vault_node::Config {});
        let vault_node_api = Arc::new(vault_node_api);

        match Quoter::run(port, vault_node_api, database).await {
            Ok(_) => info!("Stopping Blockswap Quoter"),
            Err(e) => error!("Blockswap Quoter stopped due to error: {}", e),
        }
    } else {
        eprintln!("Specified invalid port: {}", port);
    }
}
