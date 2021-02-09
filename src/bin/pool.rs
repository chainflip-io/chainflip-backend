use chainflip::{
    common::PoolCoin,
    local_store::{ILocalStore, PersistentLocalStore},
};
use chainflip_common::types::{chain::PoolChange, coin::Coin};
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
        .arg(Arg::with_name("oxen_depth").help("The change of oxen depth of oxen in decimal value"))
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

    let oxen_depth = matches
        .value_of("oxen_depth")
        .expect("Expected a oxen depth");
    let oxen_depth = oxen_depth.parse::<i128>().unwrap();
    let oxen_depth = oxen_depth
        .checked_mul(10i128.pow(Coin::OXEN.get_info().decimals))
        .expect("Failed to calculate atomic value of oxen depth");

    let pool_change = PoolChange::new(pool_coin.get_coin(), depth, oxen_depth, None);

    // Insert events into the local store
    let mut l_store = PersistentLocalStore::open("local_store.db");
    l_store
        .add_events(vec![pool_change.clone().into()])
        .unwrap();
}
