//! Implements subxt support for the reputation pallet

use std::marker::PhantomData;

use codec::Encode;
use substrate_subxt::{module, system::System, Call};

#[module]
pub trait Reputation: System {}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct Heartbeat<T: Reputation> {
    _runtime: PhantomData<T>,
}
