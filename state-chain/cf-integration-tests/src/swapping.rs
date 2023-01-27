//! Contains tests related to liquidity, pools and swapping
use frame_support::{
	assert_noop, assert_ok,
	traits::{Hooks, OnNewAccount},
};
use sp_core::H160;
use state_chain_runtime::{
	chainflip::address_derivation::AddressDerivation, AccountRoles, EpochInfo,
	EthereumIngressEgress, EthereumInstance, LiquidityPools, LiquidityProvider, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, Swapping, System, Validator, Weight, Witnesser,
};

use cf_primitives::{
	chains::{
		assets::{any, eth},
		Ethereum,
	},
	AccountId, AccountRole, AmmRange, Asset, AssetAmount, ForeignChain, ForeignChainAddress,
	PoolAssetMap,
};
use cf_traits::{AddressDerivationApi, LiquidityPoolApi, LpProvisioningApi, SwappingApi};
use pallet_cf_ingress_egress::IngressWitness;

const RANGE: AmmRange = AmmRange { lower: -100_000, upper: 100_000 };
const EGRESS_ADDRESS: [u8; 20] = [1u8; 20];
const LP: [u8; 32] = [0xF1; 32];
// Initialize exchange rate at 1:10 ratio. 1.0001^23028 = 10.001
const INITIAL_ETH_TICK: i32 = 23_028;
// Initialize exchange rate at 1:2 ratio. 1.0001^6932 = 2.00003
const INITIAL_FLIP_TICK: i32 = 6_932;

fn setup_pool(pools: Vec<(Asset, AssetAmount, u32, i32)>) {
	// Register the liquidity provider account.
	let lp: AccountId = AccountId::from(LP);
	AccountRoles::on_new_account(&lp);
	assert_ok!(LiquidityProvider::register_lp_account(RuntimeOrigin::signed(lp.clone())));

	// Register the relayer account.
	let relayer: AccountId = AccountId::from([0xE0; 32]);
	AccountRoles::on_new_account(&relayer);
	assert_ok!(AccountRoles::register_account_role(
		RuntimeOrigin::signed(relayer),
		AccountRole::Relayer
	));

	// Provide liquidity to the exchange pool.
	assert_ok!(LiquidityProvider::provision_account(&lp, Asset::Usdc, 1_000_000_000_000_000));

	for (asset, liquidity, fee, initial_tick) in pools {
		assert_ok!(LiquidityProvider::provision_account(&lp, asset, liquidity));
		assert_ok!(LiquidityPools::new_pool(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			asset,
			fee,
			initial_tick,
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			asset,
			RANGE,
			liquidity,
		));
	}
}

fn ingress_asset_for_swap(asset: eth::Asset, amount: AssetAmount) -> H160 {
	let ingress_address = <AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
		asset,
		pallet_cf_ingress_egress::IntentIdCounter::<Runtime, EthereumInstance>::get(),
	)
	.expect("Should be able to generate a valid eth address.");

	System::assert_has_event(RuntimeEvent::EthereumIngressEgress(
		pallet_cf_ingress_egress::Event::StartWitnessing { ingress_address, ingress_asset: asset },
	));

	// Define the ingress call
	let ingress_call =
		Box::new(RuntimeCall::EthereumIngressEgress(pallet_cf_ingress_egress::Call::do_ingress {
			ingress_witnesses: vec![IngressWitness {
				ingress_address,
				asset,
				amount,
				tx_id: Default::default(),
			}],
		}));

	// Get the current authorities to witness the ingress.
	let nodes = Validator::current_authorities();
	let current_epoch = Validator::current_epoch();
	for node in &nodes {
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(node.clone()),
			ingress_call.clone(),
			current_epoch
		));
	}

	ingress_address
}

#[test]
fn can_provide_liquidity_and_swap_assets() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 3_000_000u128, 0u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1_200_000u128, 0u32, 6932),
		]);

		let lp: AccountId = AccountId::from(LP);
		let relayer: AccountId = AccountId::from([0xE0; 32]);

		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Usdc),
			Some(999_999_988_843_927)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Eth),
			Some(2_071_582)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Flip),
			Some(359_567)
		);

		// Test swap
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(relayer),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(EGRESS_ADDRESS),
			0u16,
		));

		let swap_amount = 10_000u128;
		let ingress_address = ingress_asset_for_swap(eth::Asset::Eth, swap_amount);

		System::assert_has_event(RuntimeEvent::EthereumIngressEgress(
			pallet_cf_ingress_egress::Event::IngressCompleted {
				ingress_address,
				asset: eth::Asset::Eth,
				amount: swap_amount,
				tx_id: Default::default(),
			},
		));

		System::assert_has_event(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapIngressReceived {
				ingress_address: ForeignChainAddress::Eth(ingress_address.to_fixed_bytes()),
				swap_id: pallet_cf_swapping::SwapIdCounter::<Runtime>::get(),
				ingress_amount: swap_amount,
			},
		));

		// Performs the actual swap during on_idle hooks.
		let _ = Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		//  Eth: $1 <-> Flip: $5,
		// 10_000 Eth -> 100_000 USDC -> about 50_000 Flips, reduced by slippage.
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Eth,
				to: Asset::Usdc,
				input: 10_000,
				output: 98_966,
				liquidity_fee: 0,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Usdc,
				to: Asset::Flip,
				input: 98_966,
				output: 46_755,
				liquidity_fee: 0,
			},
		));

		System::assert_has_event(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapEgressScheduled {
				swap_id: 1,
				egress_id: (ForeignChain::Ethereum, 1),
				asset: Asset::Flip,
				amount: 46_755,
			},
		));

		// Egress the asset out during on_idle.
		let _ = EthereumIngressEgress::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		System::assert_has_event(RuntimeEvent::EthereumIngressEgress(
			pallet_cf_ingress_egress::Event::BatchBroadcastRequested {
				broadcast_id: 1,
				egress_ids: vec![(ForeignChain::Ethereum, 1)],
			},
		));
	});
}

#[test]
fn swap_can_accrue_fees() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 3_000_000u128, 500_000u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1_200_000u128, 500_000u32, 6932),
		]);

		let lp: AccountId = AccountId::from(LP);
		let relayer: AccountId = AccountId::from([0xE0; 32]);

		// Test swap
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(relayer),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(EGRESS_ADDRESS),
			0u16,
		));

		let swap_amount = 10_000u128;
		let _ = ingress_asset_for_swap(eth::Asset::Eth, swap_amount);

		System::reset_events();
		// Performs the actual swap during on_idle hooks.
		let _ = Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		//  Eth: $1 <-> Flip: $5,
		// 10_000 Eth -50% -> 50_000 USDC - 50% -> about 12_500 Flips, reduced by slippage.
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Eth,
				to: Asset::Usdc,
				input: 10_000,
				output: 49_742,
				liquidity_fee: 5_000,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Usdc,
				to: Asset::Flip,
				input: 49_742,
				output: 12_255,
				liquidity_fee: 24_871,
			},
		));

		System::reset_events();

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			any::Asset::Eth,
			RANGE,
			1_500_000u128
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			any::Asset::Flip,
			RANGE,
			600_000u128
		));

		// Burning half of the liquidity returns about half of assets vested.
		// All fees earned so far are also returned.
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp.clone(),
				asset: any::Asset::Eth,
				range: RANGE,
				burnt_liquidity: 1_500_000,
				assets_returned: PoolAssetMap::new(466_708, 4_708_672),
				fees_harvested: PoolAssetMap::new(4999, 0),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp.clone(),
				asset: any::Asset::Flip,
				range: RANGE,
				burnt_liquidity: 600_000,
				assets_returned: PoolAssetMap::new(414_088, 856_927),
				fees_harvested: PoolAssetMap::new(0, 24_870),
			},
		));

		// Accounts should be credited with returned capital + fees.
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp.clone(),
				asset: any::Asset::Eth,
				amount_credited: 466_708 + 4999,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp.clone(),
				asset: any::Asset::Usdc,
				amount_credited: 4_708_672,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp.clone(),
				asset: any::Asset::Usdc,
				amount_credited: 856_927 + 24_870,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp.clone(),
				asset: any::Asset::Flip,
				amount_credited: 414_088,
			},
		));

		System::reset_events();

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			any::Asset::Eth,
			RANGE,
			0u128
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			any::Asset::Flip,
			RANGE,
			0u128
		));

		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp.clone(),
				asset: any::Asset::Eth,
				range: RANGE,
				burnt_liquidity: 1_500_000,
				assets_returned: PoolAssetMap::new(466_708, 4_708_672),
				fees_harvested: PoolAssetMap::new(0, 0),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp.clone(),
				asset: any::Asset::Flip,
				range: RANGE,
				burnt_liquidity: 600_000,
				assets_returned: PoolAssetMap::new(414_088, 856_927),
				fees_harvested: PoolAssetMap::new(0, 0),
			},
		));

		// All vested assets are returned. Some swapped and with Fees added.
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Eth),
			Some(3_009_997)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Usdc),
			Some(999_999_999_999_995)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Flip),
			Some(1_187_743)
		);
	});
}

#[test]
fn swap_fails_with_insufficient_liquidity() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 1u128, 0u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1u128, 0u32, INITIAL_FLIP_TICK),
		]);

		let lp: AccountId = AccountId::from(LP);
		let relayer: AccountId = AccountId::from([0xE0; 32]);

		// Test swap
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(relayer),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(EGRESS_ADDRESS),
			0u16,
		));

		let swap_amount = 10_000u128;
		let _ = ingress_asset_for_swap(eth::Asset::Eth, swap_amount);

		assert_eq!(LiquidityPools::current_tick(&Asset::Flip), Some(INITIAL_FLIP_TICK));
		assert_eq!(LiquidityPools::current_tick(&Asset::Eth), Some(INITIAL_ETH_TICK));

		System::reset_events();

		// Swaps should fail to execute due to insufficient liquidity.
		let _ = Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));

		System::assert_last_event(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::BatchSwapFailed { asset_pair: (Asset::Eth, Asset::Flip) },
		));

		// Failed swaps should leave the pool unchanged.
		assert_eq!(LiquidityPools::current_tick(&Asset::Flip), Some(INITIAL_FLIP_TICK));
		assert_eq!(LiquidityPools::current_tick(&Asset::Eth), Some(INITIAL_ETH_TICK));

		// Failed tests are put back into the SwapQueue, and will be tried again
		System::reset_events();
		let _ = Swapping::on_idle(1, Weight::from_ref_time(1_000_000_000_000));
		System::assert_last_event(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::BatchSwapFailed { asset_pair: (Asset::Eth, Asset::Flip) },
		));
		assert_eq!(LiquidityPools::current_tick(&Asset::Flip), Some(INITIAL_FLIP_TICK));
		assert_eq!(LiquidityPools::current_tick(&Asset::Eth), Some(INITIAL_ETH_TICK));

		// Provide more liquidity for the pools
		assert_ok!(LiquidityProvider::provision_account(&lp, Asset::Eth, 1_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp, Asset::Usdc, 10_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp, Asset::Flip, 1_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp, Asset::Usdc, 2_000_000));

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			any::Asset::Eth,
			RANGE,
			3_000_000u128,
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp),
			any::Asset::Flip,
			RANGE,
			1_200_000u128,
		));

		System::reset_events();
		// Swap can now happen with sufficient liquidity
		let _ = Swapping::on_idle(2, Weight::from_ref_time(1_000_000_000_000));

		// The swap should move the tick value of the pools
		assert_eq!(LiquidityPools::current_tick(&Asset::Flip), Some(8_065));
		assert_eq!(LiquidityPools::current_tick(&Asset::Eth), Some(22_818));

		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Usdc,
				to: Asset::Flip,
				input: 98_966,
				output: 46_755,
				liquidity_fee: 0,
			},
		));

		System::assert_has_event(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapEgressScheduled {
				swap_id: 1,
				egress_id: (ForeignChain::Ethereum, 1),
				asset: Asset::Flip,
				amount: 46_755,
			},
		));
	});
}

#[test]
fn swap_fails_with_appropriate_error() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 1_000u128, 0u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1_000u128, 0u32, INITIAL_FLIP_TICK),
		]);

		assert_noop!(
			LiquidityPools::swap(Asset::Eth, Asset::Flip, 1_000_000_000u128),
			pallet_cf_pools::Error::<Runtime>::InsufficientLiquidity
		);

		assert_noop!(
			LiquidityPools::swap(Asset::Dot, Asset::Dot, 1_000_000_000u128),
			pallet_cf_pools::Error::<Runtime>::PoolDoesNotExist
		);

		assert_ok!(LiquidityPools::update_pool_enabled(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			Asset::Eth,
			false,
		));

		assert_noop!(
			LiquidityPools::swap(Asset::Eth, Asset::Usdc, 1_000_000_000u128),
			pallet_cf_pools::Error::<Runtime>::PoolDisabled
		);
	});
}
