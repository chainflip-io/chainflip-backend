#![feature(ip)]
#![feature(result_flattening)]

pub mod common;
pub mod constants;
pub mod health;
pub mod multisig;
pub mod p2p;
pub mod settings;
pub mod state_chain_observer;
pub mod stream_utils;
pub mod task_scope;
pub mod witnesser;

#[macro_use]
mod testing;

// Blockchains
pub mod btc;
pub mod dot;
pub mod eth;

pub mod logging;
