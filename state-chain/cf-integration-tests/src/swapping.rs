//! Contains tests related to liquidity, pools and swapping
use cf_amm::{
	common::{sqrt_price_at_tick, Side, SqrtPriceQ64F96, Tick},
	range_orders::Liquidity,
};
use cf_chains::{CcmIngressMetadata, Chain, Ethereum, ForeignChain, ForeignChainAddress};
use cf_primitives::{AccountId, AccountRole, Asset, AssetAmount};
use cf_test_utilities::{assert_events_eq, assert_events_match};
use cf_traits::{AddressDerivationApi, EpochInfo, LpBalanceApi};
use frame_support::{
	assert_ok,
	traits::{OnIdle, OnNewAccount},
};
use pallet_cf_ingress_egress::IngressWitness;
use state_chain_runtime::{
	chainflip::address_derivation::AddressDerivation, AccountRoles, EthereumInstance,
	LiquidityPools, LiquidityProvider, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, Swapping,
	System, Validator, Weight, Witnesser,
};

const DORIS: AccountId = AccountId::new([0x11; 32]);
const ZION: AccountId = AccountId::new([0x22; 32]);

fn new_pool(unstable_asset: Asset, fee_hundredth_pips: u32, initial_sqrt_price: SqrtPriceQ64F96) {
	assert_ok!(LiquidityPools::new_pool(
		pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
		unstable_asset,
		fee_hundredth_pips,
		initial_sqrt_price,
	));
	assert_events_eq!(
		Runtime,
		RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::NewPoolCreated {
			unstable_asset,
			fee_hundredth_pips,
			initial_sqrt_price,
		},)
	);
	System::reset_events();
}

fn new_account(account_id: &AccountId, role: AccountRole) {
	AccountRoles::on_new_account(account_id);
	assert_ok!(AccountRoles::register_account_role(
		RuntimeOrigin::signed(account_id.clone()),
		role
	));
	assert_events_eq!(
		Runtime,
		RuntimeEvent::AccountRoles(pallet_cf_account_roles::Event::AccountRoleRegistered {
			account_id: account_id.clone(),
			role,
		})
	);
	System::reset_events();
}

fn credit_account(account_id: &AccountId, asset: Asset, amount: AssetAmount) {
	let original_amount =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, asset).unwrap_or_default();
	assert_ok!(LiquidityProvider::try_credit_account(account_id, asset, amount));
	assert_eq!(
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, asset).unwrap_or_default(),
		original_amount + amount
	);
	assert_events_eq!(
		Runtime,
		RuntimeEvent::LiquidityProvider(pallet_cf_lp::Event::AccountCredited {
			account_id: account_id.clone(),
			asset,
			amount_credited: amount,
		},)
	);
	System::reset_events();
}

fn mint_range_order(
	account_id: &AccountId,
	unstable_asset: Asset,
	range: core::ops::Range<Tick>,
	liquidity: Liquidity,
) {
	let unstable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, unstable_asset).unwrap_or_default();
	let stable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, pallet_cf_pools::STABLE_ASSET)
			.unwrap_or_default();
	assert_ok!(LiquidityPools::collect_and_mint_range_order(
		RuntimeOrigin::signed(account_id.clone()),
		unstable_asset,
		range,
		liquidity,
	));
	let new_unstable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, unstable_asset).unwrap_or_default();
	let new_stable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, pallet_cf_pools::STABLE_ASSET)
			.unwrap_or_default();

	assert!(
		new_unstable_balance < unstable_balance && new_stable_balance <= stable_balance ||
			new_unstable_balance <= unstable_balance && new_stable_balance < stable_balance
	);

	let check_balance = |asset, new_balance, old_balance| {
		if new_balance < old_balance {
			assert_events_eq!(
				Runtime,
				RuntimeEvent::LiquidityProvider(pallet_cf_lp::Event::AccountDebited {
					account_id: account_id.clone(),
					asset,
					amount_debited: old_balance - new_balance,
				},)
			);
		}
	};

	check_balance(unstable_asset, new_unstable_balance, unstable_balance);
	check_balance(pallet_cf_pools::STABLE_ASSET, new_stable_balance, stable_balance);

	System::reset_events();
}

fn mint_limit_order(
	account_id: &AccountId,
	unstable_asset: Asset,
	side: Side,
	tick: Tick,
	amount: AssetAmount,
) {
	let unstable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, unstable_asset).unwrap_or_default();
	let stable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, pallet_cf_pools::STABLE_ASSET)
			.unwrap_or_default();
	assert_ok!(LiquidityPools::collect_and_mint_limit_order(
		RuntimeOrigin::signed(account_id.clone()),
		unstable_asset,
		side,
		tick,
		amount,
	));
	let new_unstable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, unstable_asset).unwrap_or_default();
	let new_stable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, pallet_cf_pools::STABLE_ASSET)
			.unwrap_or_default();

	if side == Side::Zero {
		assert_eq!(new_unstable_balance, unstable_balance - amount);
		assert_eq!(new_stable_balance, stable_balance);
	} else {
		assert_eq!(new_unstable_balance, unstable_balance);
		assert_eq!(new_stable_balance, stable_balance - amount);
	}

	let check_balance = |asset, new_balance, old_balance| {
		if new_balance < old_balance {
			assert_events_eq!(
				Runtime,
				RuntimeEvent::LiquidityProvider(pallet_cf_lp::Event::AccountDebited {
					account_id: account_id.clone(),
					asset,
					amount_debited: old_balance - new_balance,
				},)
			);
		}
	};

	check_balance(unstable_asset, new_unstable_balance, unstable_balance);
	check_balance(pallet_cf_pools::STABLE_ASSET, new_stable_balance, stable_balance);

	System::reset_events();
}

#[test]
fn basic_pool_setup_provision_and_swap() {
	super::genesis::default().build().execute_with(|| {
		new_pool(Asset::Eth, 0u32, sqrt_price_at_tick(0));
		new_pool(Asset::Flip, 0u32, sqrt_price_at_tick(0));

		new_account(&DORIS, AccountRole::LiquidityProvider);
		credit_account(&DORIS, Asset::Eth, 1_000_000);
		credit_account(&DORIS, Asset::Flip, 1_000_000);
		credit_account(&DORIS, Asset::Usdc, 1_000_000);

		mint_limit_order(&DORIS, Asset::Eth, Side::Zero, 0, 500_000);
		mint_range_order(&DORIS, Asset::Eth, -10..10, 1_000_000);

		mint_limit_order(&DORIS, Asset::Flip, Side::Zero, 0, 500_000);
		mint_range_order(&DORIS, Asset::Flip, -10..10, 1_000_000);

		new_account(&ZION, AccountRole::Relayer);

		let egress_address = [1u8; 20];

		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(ZION.clone()),
			Asset::Eth,
			Asset::Flip,
			ForeignChainAddress::Eth(egress_address),
			0u16,
			None,
		));

		let ingress_address = <AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			cf_chains::eth::assets::eth::Asset::Eth,
			pallet_cf_ingress_egress::IntentIdCounter::<Runtime, EthereumInstance>::get(),
		).unwrap();
		assert_events_eq!(Runtime, RuntimeEvent::EthereumIngressEgress(
			pallet_cf_ingress_egress::Event::StartWitnessing { ingress_address, ingress_asset: cf_chains::eth::assets::eth::Asset::Eth },
		));
		System::reset_events();

		let current_epoch = Validator::current_epoch();
		for node in Validator::current_authorities() {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(node),
				Box::new(RuntimeCall::EthereumIngressEgress(pallet_cf_ingress_egress::Call::do_ingress {
					ingress_witnesses: vec![IngressWitness {
						ingress_address,
						asset: cf_chains::eth::assets::eth::Asset::Eth,
						amount: 50,
						tx_id: Default::default(),
					}],
				})),
				current_epoch
			));
		}

		let swap_id = assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapIngressReceived {
			swap_id,
			ingress_address: ForeignChainAddress::Eth(events_ingress_address),
			ingress_amount: 50,
			..
		}) if <Ethereum as Chain>::ChainAccount::from(events_ingress_address) == ingress_address => swap_id);

		let _ = state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			1,
			Weight::from_ref_time(1_000_000_000_000),
		);

		let ((), (), (), egress_id, ..) = assert_events_match!(
			Runtime,
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Eth,
					to: Asset::Usdc,
					..
				},
			) => (),
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Usdc,
					to: Asset::Flip,
					..
				},
			) => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id: executed_swap_id,
				},
			) if executed_swap_id == swap_id => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapEgressScheduled {
					egress_id: egress_id @ (ForeignChain::Ethereum, _),
					asset: Asset::Flip,
					..
				},
			) => egress_id
		);

		assert_events_match!(
			Runtime,
			RuntimeEvent::EthereumIngressEgress(
				pallet_cf_ingress_egress::Event::BatchBroadcastRequested {
					ref egress_ids,
					..
				},
			) if egress_ids.contains(&egress_id) => ()
		);
	});
}

#[test]
fn can_process_ccm_via_swap_intent() {
	super::genesis::default().build().execute_with(|| {
		// Setup pool and liquidity
		new_pool(Asset::Eth, 0u32, sqrt_price_at_tick(0));
		new_pool(Asset::Flip, 0u32, sqrt_price_at_tick(0));

		new_account(&DORIS, AccountRole::LiquidityProvider);
		credit_account(&DORIS, Asset::Eth, 1_000_000);
		credit_account(&DORIS, Asset::Flip, 1_000_000);
		credit_account(&DORIS, Asset::Usdc, 1_000_000);
		mint_range_order(&DORIS, Asset::Eth, -1_000..1_000, 1_000_000);
		mint_range_order(&DORIS, Asset::Flip, -1_000..1_000, 1_000_000);

		new_account(&ZION, AccountRole::Relayer);

		let refund_address = ForeignChainAddress::Eth([1u8; 20]);
		let message = CcmIngressMetadata {
			message: vec![0u8, 1u8, 2u8, 3u8, 4u8],
			gas_budget: 100,
			refund_address
		};

		let egress_address = [1u8; 20];
		assert_ok!(Swapping::register_swap_intent(
			RuntimeOrigin::signed(ZION.clone()),
			Asset::Flip,
			Asset::Usdc,
			ForeignChainAddress::Eth(egress_address),
			0u16,
			Some(message),
		));

		let ingress_address = <AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			cf_chains::eth::assets::eth::Asset::Flip,
			pallet_cf_ingress_egress::IntentIdCounter::<Runtime, EthereumInstance>::get(),
		).unwrap();
		let current_epoch = Validator::current_epoch();
		for node in Validator::current_authorities() {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(node),
				Box::new(RuntimeCall::EthereumIngressEgress(pallet_cf_ingress_egress::Call::do_ingress {
					ingress_witnesses: vec![IngressWitness {
						ingress_address,
						asset: cf_chains::eth::assets::eth::Asset::Flip,
						amount: 1_000,
						tx_id: Default::default(),
					}],
				})),
				current_epoch
			));
		}

		let (principal_swap_id, gas_swap_id) = assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::CcmIngressReceived {
			ccm_id: 1,
			principal_swap_id: Some(principal_swap_id),
			gas_swap_id: Some(gas_swap_id),
			ingress_amount: 1000,
			..
		}) => (principal_swap_id, gas_swap_id));

		let _ = state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			1,
			Weight::from_ref_time(1_000_000_000_000),
		);

		let ((), (), (), (), (), egress_id) = assert_events_match!(
			Runtime,
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Flip,
					to: Asset::Usdc,
					input_amount: 100,
					..
				},
			) => (),
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Usdc,
					to: Asset::Eth,
					..
				},
			) => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id,
				},
			) if swap_id == gas_swap_id => (),
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Flip,
					to: Asset::Usdc,
					..
				},
			) => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id,
				},
			) if swap_id == principal_swap_id => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::CcmEgressScheduled {
					ccm_id: 1,
					egress_id: egress_id @ (ForeignChain::Ethereum, _),
				},
			) => egress_id
		);

		assert_events_match!(
			Runtime,
			RuntimeEvent::EthereumIngressEgress(
				pallet_cf_ingress_egress::Event::CcmBroadcastRequested {
					egress_id: actual_egress_id,
					..
				},
			) if actual_egress_id == egress_id => ()
		);
	});
}
