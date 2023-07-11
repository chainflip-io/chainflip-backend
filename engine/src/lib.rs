#![feature(ip)]
#![feature(result_flattening)]
#![feature(btree_drain_filter)]
#![feature(drain_filter)]
#![feature(map_try_insert)]
#![feature(step_trait)]
#![feature(result_option_inspect)]
#![feature(is_some_and)]

pub mod common;
pub mod constants;
pub mod db;
mod health;
mod multisig;
mod p2p;
pub mod rpc_retrier;
pub mod settings;
pub mod state_chain_observer;
pub mod stream_utils;
pub mod witness;
mod witnesser;

// Blockchains
pub mod btc;
pub mod dot;
pub mod eth;

mod start;
pub use start::start;
