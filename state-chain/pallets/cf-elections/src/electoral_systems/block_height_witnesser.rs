use core::{iter::Step, ops::RangeInclusive};

use crate::electoral_systems::state_machine::core::def_derive;

use super::{
	block_witnesser::state_machine::HookTypeFor,
	state_machine::core::{defx, Hook, HookType, Serde, Validate},
};
use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use derive_where::derive_where;
use generic_typeinfo_derive::GenericTypeInfo;
use primitives::NonemptyContinuousHeaders;
#[cfg(test)]
use proptest::prelude::Arbitrary;
#[cfg(test)]
use proptest_derive::Arbitrary;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::fmt::Debug;

pub mod consensus;
pub mod primitives;
pub mod state_machine;

#[cfg(test)]
pub trait TestTraits = Send + Sync;
#[cfg(not(test))]
pub trait TestTraits = core::any::Any;

#[cfg(test)]
pub trait MaybeArbitrary = proptest::prelude::Arbitrary + Send + Sync
where <Self as Arbitrary>::Strategy: Clone + Sync + Send;
#[cfg(not(test))]
pub trait MaybeArbitrary = core::any::Any;

pub trait CommonTraits = Debug + Clone + Encode + Decode + Serde + Eq + TypeInfo;

pub trait ChainBlockNumberTrait = CommonTraits
	+ SaturatingStep
	+ Step
	+ Default
	+ Copy
	+ Ord
	+ 'static
	+ Sized
	+ Validate
	+ MaybeArbitrary;
pub trait ChainBlockHashTrait = CommonTraits + Validate + Ord + 'static + MaybeArbitrary;

pub trait ChainTypes: Ord + Clone + Debug + 'static {
	type ChainBlockNumber: ChainBlockNumberTrait;
	type ChainBlockHash: ChainBlockHashTrait;

	const NAME: &'static str;
}
pub type ChainBlockNumberOf<T> = <T as ChainTypes>::ChainBlockNumber;
pub type ChainBlockHashOf<T> = <T as ChainTypes>::ChainBlockHash;

pub trait BHWTypes: Ord + Clone + Debug + Sized + 'static {
	type Chain: ChainTypes;
	type BlockHeightChangeHook: Hook<HookTypeFor<Self, BlockHeightChangeHook>> + CommonTraits;
	type ReorgHook: Hook<HookTypeFor<Self, ReorgHook>> + CommonTraits;
}

pub struct BlockHeightChangeHook;
impl<T: BHWTypes> HookType for HookTypeFor<T, BlockHeightChangeHook> {
	type Input = ChainBlockNumberOf<T::Chain>;
	type Output = ();
}

pub struct ReorgHook;
impl<T: BHWTypes> HookType for HookTypeFor<T, ReorgHook> {
	type Input = (ChainBlockNumberOf<T::Chain>, ChainBlockNumberOf<T::Chain>);
	type Output = ();
}

defx! {
	#[derive(GenericTypeInfo)]
	#[expand_name_with(T::Chain::NAME)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct HeightWitnesserProperties[T: BHWTypes] {
		/// An election starts with a given block number,
		/// meaning that engines have to submit all blocks they know of starting with this height.
		pub witness_from_index: <T::Chain as ChainTypes>::ChainBlockNumber,
	}
	validate _this (else HeightWitnesserPropertiesError) {}
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct BlockHeightWitnesserSettings {
		/// IMPORTANT: This value should always be greater than any reorg depth we expect to happen.
		/// If we expect reorgs of at most depth 3, set this value to over 2 times that number, so let's
		/// say 8.
		///
		/// This setting determines the number of blocks we store in the BHW to infer the depth of reorgs.
		///
		/// If you change this value, you should also look the `safety_buffer` setting of the BlockWitnesser.
		///
		/// Changing it at runtime is possible and should not have unintended consequences.
		pub safety_buffer: u32
	}
}

defx! {
	#[derive(GenericTypeInfo)]
	#[expand_name_with(T::NAME)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ChainProgress[T: ChainTypes] {
		pub headers: NonemptyContinuousHeaders<T>,
		pub removed: Option<RangeInclusive<<T as ChainTypes>::ChainBlockNumber>>,
	}
	validate _this (else ChainProgressError) {}
}
