use crate::mock::*;
use cf_primitives::{Asset, ForeignChain, ForeignChainAddress, ForeignChainAsset};
use frame_support::assert_ok;

#[test]
fn request_swap_intent() {
	new_test_ext().execute_with(|| {
		assert_ok!(Relayer::register_swap_intent(
			Origin::signed(ALICE),
			ForeignChainAsset { chain: ForeignChain::Eth, asset: Asset::Eth },
			ForeignChainAsset { chain: ForeignChain::Eth, asset: Asset::Usdc },
			ForeignChainAddress::Eth(Default::default()),
			0,
		));
	});
}
