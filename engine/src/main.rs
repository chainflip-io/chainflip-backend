use std::collections::HashMap;

extern crate config;

mod mq;
mod p2p;

fn main() {
    let mut config_options = config::Config::default();
    config_options
        .merge(config::File::with_name("../config/Default"))
        .expect("Could load default config");

    println!(
        "{:?}",
        config_options
            .try_into::<HashMap<String, String>>()
            .unwrap()
    );
}
