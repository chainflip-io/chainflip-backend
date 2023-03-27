//! Contains tests related to liquidity, pools and swapping
use cf_chains::address::ForeignChainAddress;
use cf_test_utilities::{assert_has_event_pattern, extract_from_event};
use frame_support::{
	assert_noop, assert_ok,
	traits::{OnIdle, OnNewAccount},
};
use sp_core::H160;
use state_chain_runtime::{
	chainflip::address_derivation::AddressDerivation, AccountRoles, EpochInfo, EthereumInstance,
	LiquidityPools, LiquidityProvider, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, Swapping,
	System, Validator, Weight, Witnesser,
};

use cf_primitives::{
	chains::{
		assets::{any, eth},
		Ethereum,
	},
	AccountId, AccountRole, AmmRange, Asset, AssetAmount, ForeignChain, PoolAssetMap,
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

fn setup_pool(pools: Vec<(Asset, AssetAmount, AssetAmount, u32, i32)>) {
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

	for (asset, liquidity, stable_amount, fee, initial_tick) in pools {
		assert_ok!(LiquidityProvider::provision_account(&lp, asset, liquidity));
		assert_ok!(LiquidityProvider::provision_account(&lp, Asset::Usdc, stable_amount));

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

fn do_swap_ingress(asset: eth::Asset, amount: AssetAmount) -> H160 {
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
fn can_setup_pool_and_provide_liquidity() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 3_000_000u128, 30_000_000u128, 0u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1_200_000u128, 2_400_000u128, 0u32, INITIAL_FLIP_TICK),
		]);

		let lp: AccountId = AccountId::from(LP);
		// Funds were debited to provide for liquidity.
		assert!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Usdc).unwrap() <
				30_000_000 + 2_400_000
		);
		assert!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Eth).unwrap() < 30_000_000
		);
		assert!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Flip).unwrap() < 12_000_000
		);

		assert_eq!(LiquidityPools::minted_liquidity(&lp, &any::Asset::Eth, RANGE), 3_000_000u128);
		assert_eq!(LiquidityPools::minted_liquidity(&lp, &any::Asset::Flip, RANGE), 1_200_000u128);
	});
}

#[test]
fn can_swap_assets() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 3_000_000u128, 30_000_000u128, 0u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1_200_000u128, 2_400_000u128, 0u32, INITIAL_FLIP_TICK),
		]);

		let relayer: AccountId = AccountId::from([0xE0; 32]);

		// Test swap
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(relayer),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(EGRESS_ADDRESS),
			0u16,
		));

		const SWAP_AMOUNT: u128 = 10_000;
		let expected_ingress_address = do_swap_ingress(eth::Asset::Eth, SWAP_AMOUNT);

		let (swap_id, ingress_address) = extract_from_event!(
			RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapIngressReceived {
				swap_id,
				ingress_address,
				ingress_amount: SWAP_AMOUNT,
			}) => (swap_id, ingress_address)
		);
		assert_eq!(ingress_address, expected_ingress_address.into());

		// Performs the actual swap during on_idle hooks.
		let _ = state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			1,
			Weight::from_ref_time(1_000_000_000_000),
		);

		//  Eth: $1 <-> Flip: $5,
		// 10_000 Eth -> 100_000 USDC -> about 50_000 Flips, reduced by slippage.
		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Eth,
				to: Asset::Usdc,
				input: 10_000,
				liquidity_fee: 0,
				..
			},
		));
		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Usdc,
				to: Asset::Flip,
				liquidity_fee: 0,
				..
			},
		));

		assert_has_event_pattern!(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapExecuted {
				swap_id: executed_swap_id,
			},
		) if executed_swap_id == swap_id);

		let egress_id = extract_from_event!(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapEgressScheduled {
				egress_id: egress_id @ (ForeignChain::Ethereum, _),
				asset: Asset::Flip,
				..
			},
		) => egress_id);

		assert_has_event_pattern!(RuntimeEvent::EthereumIngressEgress(
			pallet_cf_ingress_egress::Event::BatchBroadcastRequested {
				ref egress_ids,
				..
			},
		) if egress_ids.contains(&egress_id));
	});
}

#[test]
fn swap_can_accrue_fees() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 3_000_000u128, 30_000_000u128, 500_000u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1_200_000u128, 2_400_000u128, 500_000u32, INITIAL_FLIP_TICK),
		]);

		let lp: AccountId = AccountId::from(LP);
		let relayer: AccountId = AccountId::from([0xE0; 32]);

		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(relayer),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(EGRESS_ADDRESS),
			0u16,
		));

		const SWAP_AMOUNT: u128 = 10_000u128;
		let _ = do_swap_ingress(eth::Asset::Eth, SWAP_AMOUNT);

		System::reset_events();

		// Performs the actual swap during on_idle hooks.
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			1,
			Weight::from_ref_time(1_000_000_000_000),
		);

		//  Eth: $1 <-> Flip: $5,
		// 10_000 Eth -50% -> 50_000 USDC - 50% -> about 12_500 Flip, reduced by slippage.
		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Eth,
				to: Asset::Usdc,
				input: 10_000,
				liquidity_fee: 5_000,
				..
			},
		));
		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Usdc,
				to: Asset::Flip,
				liquidity_fee: 24000..25000,
				..
			}
		));

		System::reset_events();

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			any::Asset::Eth,
			RANGE,
			0
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp.clone()),
			any::Asset::Flip,
			RANGE,
			0
		));

		// Burning the liquidity returns the assets vested.
		// All fees earned so far are also returned.
		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				asset: any::Asset::Eth,
				range: RANGE,
				fees_harvested: PoolAssetMap { asset_0: 1.., asset_1: 0 },
				..
			}
		));
		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				asset: any::Asset::Flip,
				range: RANGE,
				fees_harvested: PoolAssetMap { asset_0: 0, asset_1: 1.. },
				..
			}
		));

		// All vested assets are returned. Some swapped and with Fees added.
		// Approx: 3_000_000 + 5000 (liquidity fee) + 5000 (swap input)
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Eth),
			Some(3_009_998)
		);
		// Approx: 30_000_000 + 2_400_000
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Usdc),
			Some(32_399_996)
		);
		// Appox: 1_200_000 - (10_000 * 0.5(fee) * 0.5(fee) * 5(exchange rate))
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp, any::Asset::Flip),
			Some(1_187_744)
		);
	});
}

#[test]
fn swap_fails_with_insufficient_liquidity() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 1u128, 10u128, 0u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1u128, 2u128, 0u32, INITIAL_FLIP_TICK),
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
		let _ = do_swap_ingress(eth::Asset::Eth, swap_amount);

		assert_eq!(LiquidityPools::current_tick(&Asset::Flip), Some(INITIAL_FLIP_TICK));
		assert_eq!(LiquidityPools::current_tick(&Asset::Eth), Some(INITIAL_ETH_TICK));

		// Swaps should fail to execute due to insufficient liquidity.
		let _ = state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			1,
			Weight::from_ref_time(1_000_000_000_000),
		);

		assert_has_event_pattern!(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::BatchSwapFailed { asset_pair: (Asset::Eth, Asset::Flip) },
		));

		// Failed swaps should leave the pool unchanged.
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
		let _ = state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			2,
			Weight::from_ref_time(1_000_000_000_000),
		);

		// The swap should move the tick value of the pools
		assert_eq!(LiquidityPools::current_tick(&Asset::Flip), Some(8_065));
		assert_eq!(LiquidityPools::current_tick(&Asset::Eth), Some(22_818));

		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped {
				from: Asset::Eth,
				to: Asset::Usdc,
				input: 10_000,
				..
			},
		));
		assert_has_event_pattern!(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::AssetsSwapped { from: Asset::Usdc, to: Asset::Flip, .. },
		));

		assert_has_event_pattern!(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapEgressScheduled {
				swap_id: 1,
				egress_id: (ForeignChain::Ethereum, 1),
				asset: Asset::Flip,
				..
			},
		));
	});
}

#[test]
fn swap_fails_with_appropriate_error() {
	super::genesis::default().build().execute_with(|| {
		setup_pool(vec![
			(Asset::Eth, 1_000u128, 10_000u128, 0u32, INITIAL_ETH_TICK),
			(Asset::Flip, 1_000u128, 2_000u128, 0u32, INITIAL_FLIP_TICK),
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
