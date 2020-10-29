#[macro_use]
extern crate log;

use clap::{App, Arg};

use blockswap::logging;
use blockswap::quoter::{config::QUOTER_CONFIG, database, vault_node, Quoter};
use std::{
    net::Ipv4Addr,
    str::FromStr,
    sync::{Arc, Mutex},
};

/*
Entry point for the Quoter binary. We should try to keep it as small as posible
and implement most of the core logic as part of the library (src/lib.rs). This way
of organising code works better with integration tests.
 Ideally we would just parse commad line arguments here and call into the library.
*/

fn main() {
    std::panic::set_hook(Box::new(|msg| {
        error!("Panicked with: {}", msg);
        std::process::exit(101); // Rust's panics use 101 by default
    }));

    let matches = App::new("Chainflip Quoter")
        .version("0.1")
        .about("A web server that provides swap quotes")
        .arg(
            Arg::with_name("ip")
                .long("ip")
                .takes_value(true)
                .help("IP on which to listen for incoming connections"),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .takes_value(true)
                .help("Port on which to listen for incoming connections"),
        )
        .get_matches();

    logging::init("quoter", None);

    let ip = matches.value_of("ip").unwrap_or("127.0.0.1");
    let ipv4 = Ipv4Addr::from_str(ip).expect("Invalid ipv4 address");
    let port = matches.value_of("port").unwrap_or("3033");

    if let Ok(port) = port.parse::<u16>() {
        let config = &QUOTER_CONFIG;
        info!("Starting the Chainflip Quoter");

        let database = database::Database::open(&config.database.name);
        let database = Arc::new(Mutex::new(database));

        let vault_node_api = vault_node::VaultNodeAPI::new(&config.vault_node_url);
        let vault_node_api = Arc::new(vault_node_api);

        match Quoter::run((ipv4, port), vault_node_api, database) {
            Ok(_) => info!("Stopping Chainflip Quoter"),
            Err(e) => error!("Chainflip Quoter stopped due to error: {}", e),
        }
    } else {
        eprintln!("Specified invalid port: {}", port);
    }
}
