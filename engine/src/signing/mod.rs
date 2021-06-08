mod client;
pub mod crypto;

#[cfg(test)]
mod distributed_signing;

pub use client::MultisigClient;

pub type MessageHash = Vec<u8>;
