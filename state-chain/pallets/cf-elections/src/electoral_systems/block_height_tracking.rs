use core::{iter::Step, ops::RangeInclusive};

use super::{
	block_witnesser::state_machine::HookTypeFor,
	state_machine::core::{defx, Hook, HookType, Serde, Validate},
};
use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use derive_where::derive_where;
use primitives::{Header, NonemptyContinuousHeaders};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, fmt::Debug};

#[cfg(test)]
use proptest_derive::Arbitrary;

pub mod consensus;
pub mod primitives;
pub mod state_machine;

trait CommonTraits = Debug + Clone + Serde;

pub trait ChainBlockNumberTrait =
	SaturatingStep + Step + BlockZero + Debug + Copy + Ord + Serde + 'static + Sized + Validate;
pub trait ChainBlockHashTrait = Validate + Serde + Ord + Clone + Debug + 'static;

pub trait ChainTypes: Ord + Clone + Debug + 'static {
	type ChainBlockNumber: ChainBlockNumberTrait;
	type ChainBlockHash: ChainBlockHashTrait;

	/// IMPORTANT: this value must always be greater than the safety margin we use, and represent
	/// the buffer of data we keep around (in number of blocks) both in the ElectionTracker and in
	/// the BlockProcessor
	const SAFETY_BUFFER: u32;
}
pub type ChainBlockNumberOf<T: ChainTypes> = T::ChainBlockNumber;
pub type ChainBlockHashOf<T: ChainTypes> = T::ChainBlockHash;

pub trait HWTypes: Ord + Clone + Debug + Sized + 'static {
	type Chain: ChainTypes;
	type BlockHeightChangeHook: Hook<HookTypeFor<Self, BlockHeightChangeHook>> + CommonTraits;

	// TODO remove this one, replace by SAFETY_MARGIN
	const BLOCK_BUFFER_SIZE: usize;
}

pub struct BlockHeightChangeHook;
impl<T: HWTypes> HookType for HookTypeFor<T, BlockHeightChangeHook> {
	type Input = ChainBlockNumberOf<T::Chain>;
	type Output = ();
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive_where(
	Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd;
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct HeightWitnesserProperties<T: HWTypes> {
	/// An election starts with a given block number,
	/// meaning that engines have to submit all blocks they know of starting with this height.
	pub witness_from_index: <T::Chain as ChainTypes>::ChainBlockNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgressType {
	Continous,
	Reorg,
}

defx! {
	pub struct ChainProgress[T: ChainTypes] {
		pub headers: NonemptyContinuousHeaders<T>,
		pub removed: Option<RangeInclusive<ChainBlockNumberOf<T>>>,
	}

	validate _this (else ChainProgressError) {
		// TODO: ensure that the defx macro calls the is_valid
		// on `headers` automatically!
	}

}
pub type ChainProgressFor<T> = ChainProgress<T>;

//-------- implementation of block height tracking as a state machine --------------

pub trait BlockHeightTrait = PartialEq + Ord + Copy + Step + BlockZero;
