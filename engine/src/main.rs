mod mq;
mod p2p;
mod settings;

use settings::Settings;

fn main() {
    let settings = Settings::new().expect("Failed to initialise settings");

    println!("{:?}", settings);
}
