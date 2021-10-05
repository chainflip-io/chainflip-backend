#[macro_use]
mod utils;
#[allow(clippy::module_inception)]
mod client_inner;
mod common;
// TODO: make it unnecessary to expose macros here
#[macro_use]
mod frost;
mod frost_stages;
mod key_store;
mod keygen_data;
mod keygen_frost;
mod keygen_manager;
mod keygen_stages;
mod keygen_state;
// mod legacy_keygen_manager;
// mod legacy_keygen_state;
mod shared_secret;
mod signing_manager;
mod signing_state;

#[cfg(test)]
mod tests;

pub use client_inner::{InnerEvent, MultisigClient};

#[cfg(test)]
mod genesis;

pub use client_inner::{KeygenOutcome, SchnorrSignature, SigningOutcome};
pub use common::KeygenResultInfo;
