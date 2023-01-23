//! Contains tests related to liquidity, pools and swapping
use frame_support::{
	assert_ok,
	traits::{Hooks, OnNewAccount},
};
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
use cf_traits::{AddressDerivationApi, LpProvisioningApi};
use pallet_cf_ingress_egress::IngressWitness;

#[test]
fn can_provide_liquidity_and_swap_assets() {
	super::genesis::default().build().execute_with(|| {
		// Register the liquidity provider account.
		let lp_1: AccountId = AccountId::from([0xF1; 32]);
		let lp_2: AccountId = AccountId::from([0xF2; 32]);
		AccountRoles::on_new_account(&lp_1);
		AccountRoles::on_new_account(&lp_2);
		assert_ok!(LiquidityProvider::register_lp_account(RuntimeOrigin::signed(lp_1.clone())));
		assert_ok!(LiquidityProvider::register_lp_account(RuntimeOrigin::signed(lp_2.clone())));

		// Register the relayer account.
		let relayer: AccountId = AccountId::from([0xE0; 32]);
		AccountRoles::on_new_account(&relayer);
		assert_ok!(AccountRoles::register_account_role(
			RuntimeOrigin::signed(relayer.clone()),
			AccountRole::Relayer
		));

		let egress_address = [1u8; 20];

		// Provide liquidity to the exchange pool.
		assert_ok!(LiquidityProvider::provision_account(&lp_1, Asset::Eth, 1_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp_1, Asset::Usdc, 10_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp_2, Asset::Flip, 1_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp_2, Asset::Usdc, 2_000_000));

		// Use governance to create a new Flip <-> USDC pool.
		// Initialize exchange rate at 1:10 ratio. 1.0001^23028 = 10.001
		assert_ok!(LiquidityPools::new_pool(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			any::Asset::Eth,
			0u32,
			23_028,
		));

		// Use governance to create a new Eth <-> USDC pool.
		// Initialize exchange rate at 1:2 ratio. 1.0001^6932 = 2.00003
		assert_ok!(LiquidityPools::new_pool(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			any::Asset::Flip,
			0u32,
			6_932,
		));

		// Provide enough liquidity for the pools
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_1.clone()),
			any::Asset::Eth,
			AmmRange::new(-100_000, 100_000),
			3_000_000u128,
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_2.clone()),
			any::Asset::Flip,
			AmmRange::new(-100_000, 100_000),
			1_200_000u128,
		));

		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_1, any::Asset::Eth),
			Some(71_582)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_1, any::Asset::Usdc),
			Some(532_912)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_2, any::Asset::Flip),
			Some(159_567)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_2, any::Asset::Usdc),
			Some(311_015)
		);

		// Test swap
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(relayer),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(egress_address),
			0u16,
		));

		// Note the ingress address here
		let ingress_address =
			<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
				eth::Asset::Eth,
				pallet_cf_ingress_egress::IntentIdCounter::<Runtime, EthereumInstance>::get(),
			)
			.expect("Should be able to generate a valid eth address.");

		System::assert_has_event(RuntimeEvent::EthereumIngressEgress(
			pallet_cf_ingress_egress::Event::StartWitnessing {
				ingress_address,
				ingress_asset: eth::Asset::Eth,
			},
		));

		const SWAP_AMOUNT: AssetAmount = 10_000;
		// Define the ingress call
		let ingress_call = Box::new(RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::do_ingress {
				ingress_witnesses: vec![IngressWitness {
					ingress_address,
					asset: eth::Asset::Eth,
					amount: SWAP_AMOUNT,
					tx_id: Default::default(),
				}],
			},
		));

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

		System::assert_has_event(RuntimeEvent::EthereumIngressEgress(
			pallet_cf_ingress_egress::Event::IngressCompleted {
				ingress_address,
				asset: eth::Asset::Eth,
				amount: SWAP_AMOUNT,
				tx_id: Default::default(),
			},
		));

		System::assert_has_event(RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapIngressReceived {
				ingress_address: ForeignChainAddress::Eth(ingress_address.to_fixed_bytes()),
				swap_id: pallet_cf_swapping::SwapIdCounter::<Runtime>::get(),
				ingress_amount: SWAP_AMOUNT,
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
		// Register the liquidity provider account.
		let lp_1: AccountId = AccountId::from([0xF1; 32]);
		let lp_2: AccountId = AccountId::from([0xF2; 32]);
		AccountRoles::on_new_account(&lp_1);
		AccountRoles::on_new_account(&lp_2);
		assert_ok!(LiquidityProvider::register_lp_account(RuntimeOrigin::signed(lp_1.clone())));
		assert_ok!(LiquidityProvider::register_lp_account(RuntimeOrigin::signed(lp_2.clone())));

		// Register the relayer account.
		let relayer: AccountId = AccountId::from([0xE0; 32]);
		AccountRoles::on_new_account(&relayer);
		assert_ok!(AccountRoles::register_account_role(
			RuntimeOrigin::signed(relayer.clone()),
			AccountRole::Relayer
		));

		let egress_address = [1u8; 20];
		let range = AmmRange::new(-100_000, 100_000);

		// Provide liquidity to the exchange pool.
		assert_ok!(LiquidityProvider::provision_account(&lp_1, Asset::Eth, 1_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp_1, Asset::Usdc, 10_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp_2, Asset::Flip, 1_000_000));
		assert_ok!(LiquidityProvider::provision_account(&lp_2, Asset::Usdc, 2_000_000));

		// Use governance to create a new Flip <-> USDC pool.
		// Initialize exchange rate at 1:10 ratio. 1.0001^23028 = 10.001
		// Fee is set as 50%
		assert_ok!(LiquidityPools::new_pool(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			any::Asset::Eth,
			500000u32,
			23_028,
		));

		// Use governance to create a new Eth <-> USDC pool.
		// Initialize exchange rate at 1:2 ratio. 1.0001^6932 = 2.00003
		// Fee is set as 50%
		assert_ok!(LiquidityPools::new_pool(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			any::Asset::Flip,
			500000u32,
			6_932,
		));

		// Provide enough liquidity for the pools
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_1.clone()),
			any::Asset::Eth,
			range,
			3_000_000u128,
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_2.clone()),
			any::Asset::Flip,
			range,
			1_200_000u128,
		));

		// Test swap
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(relayer),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(egress_address),
			0u16,
		));

		// Note the ingress address here
		let ingress_address =
			<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
				eth::Asset::Eth,
				pallet_cf_ingress_egress::IntentIdCounter::<Runtime, EthereumInstance>::get(),
			)
			.expect("Should be able to generate a valid eth address.");

		const SWAP_AMOUNT: AssetAmount = 10_000;
		// Define the ingress call
		let ingress_call = Box::new(RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::do_ingress {
				ingress_witnesses: vec![IngressWitness {
					ingress_address,
					asset: eth::Asset::Eth,
					amount: SWAP_AMOUNT,
					tx_id: Default::default(),
				}],
			},
		));

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

		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_1, any::Asset::Eth),
			Some(71_582)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_1, any::Asset::Usdc),
			Some(532_912)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_2, any::Asset::Flip),
			Some(159_567)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_2, any::Asset::Usdc),
			Some(311_015)
		);

		System::reset_events();

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_1.clone()),
			any::Asset::Eth,
			range,
			1_500_000u128
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_2.clone()),
			any::Asset::Flip,
			range,
			600_000u128
		));

		// Burning half of the liquidity returns about half of assets vested.
		// All fees earned so far are also returned.
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp_1.clone(),
				asset: any::Asset::Eth,
				range,
				burnt_liquidity: 1_500_000,
				assets_returned: PoolAssetMap::new(466_708, 4_708_672),
				fees_harvested: PoolAssetMap::new(4999, 0),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp_2.clone(),
				asset: any::Asset::Flip,
				range,
				burnt_liquidity: 600_000,
				assets_returned: PoolAssetMap::new(414_088, 856_927),
				fees_harvested: PoolAssetMap::new(0, 24_870),
			},
		));

		// Accounts should be credited with returned capital + fees.
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp_1.clone(),
				asset: any::Asset::Eth,
				amount_credited: 466_708 + 4999,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp_1.clone(),
				asset: any::Asset::Usdc,
				amount_credited: 4_708_672,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp_2.clone(),
				asset: any::Asset::Usdc,
				amount_credited: 856_927 + 24_870,
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityProvider(
			pallet_cf_lp::Event::AccountCredited {
				account_id: lp_2.clone(),
				asset: any::Asset::Flip,
				amount_credited: 414_088,
			},
		));

		System::reset_events();

		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_1.clone()),
			any::Asset::Eth,
			range,
			0u128
		));
		assert_ok!(LiquidityProvider::update_position(
			RuntimeOrigin::signed(lp_2.clone()),
			any::Asset::Flip,
			range,
			0u128
		));

		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp_1.clone(),
				asset: any::Asset::Eth,
				range,
				burnt_liquidity: 1_500_000,
				assets_returned: PoolAssetMap::new(466_708, 4_708_672),
				fees_harvested: PoolAssetMap::new(0, 0),
			},
		));
		System::assert_has_event(RuntimeEvent::LiquidityPools(
			pallet_cf_pools::Event::LiquidityBurned {
				lp: lp_2.clone(),
				asset: any::Asset::Flip,
				range,
				burnt_liquidity: 600_000,
				assets_returned: PoolAssetMap::new(414_088, 856_927),
				fees_harvested: PoolAssetMap::new(0, 0),
			},
		));

		// All vested assets are returned. Some swapped and with Fees added.
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_1, any::Asset::Eth),
			Some(1_009_997)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_1, any::Asset::Usdc),
			Some(9_950_256)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_2, any::Asset::Flip),
			Some(987_743)
		);
		assert_eq!(
			pallet_cf_lp::FreeBalances::<Runtime>::get(&lp_2, any::Asset::Usdc),
			Some(2_049_739)
		);
	});
}
