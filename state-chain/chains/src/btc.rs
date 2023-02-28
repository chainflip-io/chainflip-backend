use cf_primitives::chains::{assets, Bitcoin};

use crate::Chain;

pub type BlockNumber = u64;

// TODO: Come back to this. in BTC u64 works, but the trait has from u128 required, so we do this
// for now
type Amount = u128;

impl Chain for Bitcoin {
	type ChainBlockNumber = BlockNumber;

	type ChainAmount = u128;

	// Or number of bytes??
	type TransactionFee = Amount;

	type TrackedData = ();

	type ChainAsset = assets::btc::Asset;

	// TODO: Provide an actual value for this
	type ChainAccount = u64;

	type EpochStartData = ();
}
