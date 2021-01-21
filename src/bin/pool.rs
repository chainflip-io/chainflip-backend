use chainflip::{
    common::PoolCoin,
    local_store::{ILocalStore, PersistentLocalStore},
};
use chainflip_common::types::{chain::PoolChange, coin::Coin, UUIDv4};
use clap::{App, Arg};
use std::str::FromStr;

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|msg| {
        eprintln!("Panicked with: {}", msg);
        std::process::exit(101); // Rust's panics use 101 by default
    }));

    let matches = App::new("Chainflip pool")
        .version("1.0")
        .about("Util for changing pool value in chainflip")
        .arg(Arg::with_name("coin").help("The pool coin: eth or btc"))
        .arg(Arg::with_name("depth").help("The change of depth of the coin in decimal value"))
        .arg(Arg::with_name("loki_depth").help("The change of loki depth of loki in decimal value"))
        .get_matches();

    let coin = matches
        .value_of("coin")
        .expect("Expected coin to be present");
    let coin = Coin::from_str(coin).expect("Invalid coin");
    let pool_coin = PoolCoin::from(coin).expect("Invalid pool coin");

    let depth = matches.value_of("depth").expect("Expected a depth");
    let depth = depth.parse::<i128>().unwrap();
    let depth = depth
        .checked_mul(10i128.pow(coin.get_info().decimals))
        .expect("Failed to calculate atomic value of depth");

    let loki_depth = matches
        .value_of("loki_depth")
        .expect("Expected a loki depth");
    let loki_depth = loki_depth.parse::<i128>().unwrap();
    let loki_depth = loki_depth
        .checked_mul(10i128.pow(Coin::LOKI.get_info().decimals))
        .expect("Failed to calculate atomic value of loki depth");

    let pool_change = PoolChange {
        id: UUIDv4::new(),
        pool: pool_coin.get_coin(),
        depth_change: depth,
        base_depth_change: loki_depth,
        event_number: None,
    };

    // Insert events into the local store
    let mut l_store = PersistentLocalStore::open("local_store.db");
    l_store
        .add_events(vec![pool_change.clone().into()])
        .unwrap();

    println!("Added event: {:?}", pool_change);
}
