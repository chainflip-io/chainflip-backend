//! Implements subxt support for the reputation pallet

use std::marker::PhantomData;

use cf_traits::EpochInfo;
use codec::Encode;
use frame_support::{pallet_prelude::Member, traits::IsType, Parameter};
use frame_system::Event;
use substrate_subxt::{module, system::System, Call};

#[module]
pub trait Reputation: System {}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct Heartbeat<T: Reputation> {
    _runtime: PhantomData<T>,
}
