use core::{iter::Step, ops::RangeInclusive};

use super::{
	block_witnesser::state_machine::HookTypeFor,
	state_machine::core::{Hook, HookType, Serde, Validate},
};
use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use derive_where::derive_where;
use frame_support::ensure;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, fmt::Debug};

#[cfg(test)]
use proptest_derive::Arbitrary;

pub mod consensus;
pub mod primitives;
pub mod state_machine;

pub trait ChainTypes: Ord + Clone + Debug + 'static {
	type ChainBlockNumber: SaturatingStep
		+ Step
		+ BlockZero
		+ Debug
		+ Copy
		+ Ord
		+ Serde
		+ 'static
		+ Sized;
	type ChainBlockHash: Serde + Ord + Clone + Debug + 'static;
}

pub trait HWTypes: ChainTypes {
	const BLOCK_BUFFER_SIZE: usize;

	type BlockHeightChangeHook: Hook<HookTypeFor<Self, BlockHeightChangeHook>>;
}

pub struct BlockHeightChangeHook;
impl<T: HWTypes> HookType for HookTypeFor<T, BlockHeightChangeHook> {
	type Input = T::ChainBlockNumber;
	type Output = ();
}

#[cfg_attr(test, derive(Arbitrary))]
#[derive_where(
	Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd;
	T::ChainBlockNumber: Debug + Clone + Copy + Eq + Ord
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct HeightWitnesserProperties<T: ChainTypes> {
	/// An election starts with a given block number,
	/// meaning that engines have to submit all blocks they know of starting with this height.
	pub witness_from_index: T::ChainBlockNumber,
}

#[derive_where(
	Debug, Clone, PartialEq, Eq;
	// T::ChainBlockNumber: Debug + Clone + Ord
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum ChainProgress<T: ChainTypes> {
	// Range of new block heights witnessed. If this is not consecutive, it means that
	Range(BTreeMap<T::ChainBlockNumber, T::ChainBlockHash>, RangeInclusive<T::ChainBlockNumber>),
	// Range of new block heights, only emitted when there is a consensus for the first time after
	// being started.
	// FirstConsensus(BTreeMap<T::ChainBlockNumber, T::ChainBlockHash>,
	// RangeInclusive<T::ChainBlockNumber>), there was no update to the witnessed block headers
	None,
}

impl<T: ChainTypes> Validate for ChainProgress<T> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		use ChainProgress::*;
		match self {
			Range(hashes, range) => {
				ensure!(
					range.start() <= range.end(),
					"range a..=b in ChainProgress should have a <= b"
				);
				ensure!(
					hashes.keys().all(|key| range.contains(key)),
					"hashes should all be inside the range"
				);
				ensure!(
					range.clone().all(|key| hashes.contains_key(&key)),
					"all heights should have an attached hash"
				);
				Ok(())
			},
			None => Ok(()),
		}
	}
}

//-------- implementation of block height tracking as a state machine --------------

pub trait BlockHeightTrait = PartialEq + Ord + Copy + Step + BlockZero;
