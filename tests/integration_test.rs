#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

#[cfg(test)]
mod vault_api;

#[cfg(test)]
mod witness;

#[cfg(test)]
mod processor;

#[cfg(test)]
mod eth_web3_client;