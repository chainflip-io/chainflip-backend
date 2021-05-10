use log::info;

mod mq;
mod p2p;
mod settings;

// Blockchains
mod eth;

use settings::Settings;

fn main() {
    // init the logger
    env_logger::init();

    info!("Start your engines!");

    let settings = Settings::new().expect("Failed to initialise settings");
}
