use core::marker::PhantomData;

use cf_chains::Chain;

use crate::GetBlockHeight;

use super::MockPallet;
use crate::mocks::MockPalletStorage;

pub struct BlockHeightProvider<C: Chain>(PhantomData<C>);

impl<C: Chain> MockPallet for BlockHeightProvider<C> {
	const PREFIX: &'static [u8] = b"MockBlockHeightProvider";
}

const BLOCK_HEIGHT_KEY: &[u8] = b"BLOCK_HEIGHT";

impl<C: Chain> BlockHeightProvider<C> {
	pub fn set_block_height(height: C::ChainBlockNumber) {
		Self::put_value(BLOCK_HEIGHT_KEY, height);
	}

	pub fn mutate_block_height<F>(f: F)
		where F : FnOnce(C::ChainBlockNumber) -> C::ChainBlockNumber
	{
		Self::set_block_height(f(Self::get_block_height()));
	}
}

const DEFAULT_BLOCK_HEIGHT: u32 = 1337;

impl<C: Chain> GetBlockHeight<C> for BlockHeightProvider<C> {
	fn get_block_height() -> C::ChainBlockNumber {
		Self::get_value(BLOCK_HEIGHT_KEY).unwrap_or(DEFAULT_BLOCK_HEIGHT.into())
	}
}
