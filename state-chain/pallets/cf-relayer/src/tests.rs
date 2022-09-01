use crate::{mock::*, IndexCounter, SwapIntents};
use cf_chains::{
	assets::{Asset, AssetAddress},
	eth::Address,
};
use cf_test_utilities::last_event;
use frame_support::assert_ok;

#[test]
fn request_swap_intent() {
	new_test_ext().execute_with(|| {
		assert_ok!(Relayer::request_swap_intent(
			Origin::signed(ALICE),
			(Asset::EthEth, Asset::EthEth),
			AssetAddress::ETH(Address::default()),
			0
		));
		assert_eq!(IndexCounter::<Test>::get(), 1);
		for swap_intent in SwapIntents::<Test>::iter_values() {
			assert_eq!(
				last_event::<Test>(),
				crate::mock::Event::Relayer(crate::Event::NewIngressIntent(
					swap_intent.ingress_address,
					swap_intent.tx_hash
				))
			);
		}
	});
}
