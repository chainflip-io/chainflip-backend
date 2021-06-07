mod client_inner;

mod shared_secret;
mod signing_state;
mod signing_state_manager;

#[cfg(test)]
mod tests;

pub(super) use client_inner::{InnerEvent, InnerSignal, MultisigClientInner};
