use super::state_machine::{BWElectionProperties, BWTypes};
use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::BTreeMap;

#[derive_where(Debug, Clone, PartialEq, Eq;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	T::ChainBlockNumber: Encode,
	T::ChainBlockHash: Encode,
	T::BlockData: Encode,
))]
pub struct OptimisticBlock<T: BWTypes> {
	pub hash: T::ChainBlockHash,
	pub data: T::BlockData,
}

#[derive_where(Default, Debug, Clone, PartialEq, Eq;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	T::ChainBlockNumber: Encode,
	T::ChainBlockHash: Encode,
	T::BlockData: Encode,
))]
pub struct OptimisticBlockCache<T: BWTypes> {
	blocks: BTreeMap<T::ChainBlockNumber, OptimisticBlock<T>>,
}

impl<T: BWTypes> OptimisticBlockCache<T> {
	pub fn add_block(&mut self, height: T::ChainBlockNumber, block: OptimisticBlock<T>) {
		self.blocks.insert(height, block);
	}

	pub fn get_blocks(
		&mut self,
		properties: &Vec<BWElectionProperties<T>>,
	) -> Vec<OptimisticBlock<T>> {
		// TODO this algorithm could be improved probably!
		let mut result = Vec::new();
		for property in properties {
			if let Some(block) = self.blocks.remove(&property.block_height) {
				if Some(&block.hash) == property.block_hash.as_ref() {
					result.push(block);
				} else {
					self.blocks.insert(property.block_height, block);
				}
			}
		}
		result
	}

	pub fn delete_old_blocks(&mut self, current_height: T::ChainBlockNumber, safety_margin: usize) {
		self.blocks
			.retain(|height, _| current_height <= height.saturating_forward(safety_margin));
	}
}
