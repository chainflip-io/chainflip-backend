
#[macro_use]
extern crate log;

use clap::{App, Arg};

use blockswap::quoter;
use blockswap::logging;

/*
Entry point for the Quoter binary. We should try to keep it as small as posible
and implement most of the core logic as part of the library (src/lib.rs). This way
of organising code works better with integration tests.
 Ideally we would just parse commad line arguments here and call into the library.
*/

#[tokio::main]
async fn main() {
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

    logging::init("quoter");

    let port = matches.value_of("port").unwrap_or("3033");

    if let Ok(port) = port.parse::<u16>() {

        info!("Starting the Blockswap Quoter");

        quoter::serve(port).await;
    } else {
        eprintln!("Specified invalid port: {}", port);
    }
}
