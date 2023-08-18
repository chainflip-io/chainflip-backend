//! Contains tests related to liquidity, pools and swapping
use cf_amm::{
	common::{price_at_tick, Order, Price, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, EncodedAddress},
	CcmChannelMetadata, CcmDepositMetadata, Chain, Ethereum, ForeignChain, ForeignChainAddress,
	SwapOrigin,
};
use cf_primitives::{AccountId, AccountRole, Asset, AssetAmount, STABLE_ASSET};
use cf_test_utilities::{assert_events_eq, assert_events_match};
use cf_traits::{AccountRoleRegistry, EpochInfo, GetBlockHeight, LpBalanceApi};
use frame_support::{
	assert_ok,
	traits::{OnFinalize, OnIdle, OnNewAccount},
};
use pallet_cf_ingress_egress::DepositWitness;
use pallet_cf_pools::OldRangeOrderSize;
use pallet_cf_swapping::CcmIdCounter;
use state_chain_runtime::{
	chainflip::{address_derivation::AddressDerivation, ChainAddressConverter},
	AccountRoles, EthereumInstance, LiquidityPools, LiquidityProvider, Runtime, RuntimeCall,
	RuntimeEvent, RuntimeOrigin, Swapping, System, Timestamp, Validator, Weight, Witnesser,
};

use state_chain_runtime::EthereumChainTracking;

const DORIS: AccountId = AccountId::new([0x11; 32]);
const ZION: AccountId = AccountId::new([0x22; 32]);

fn new_pool(unstable_asset: Asset, fee_hundredth_pips: u32, initial_price: Price) {
	assert_ok!(LiquidityPools::new_pool(
		pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
		unstable_asset,
		fee_hundredth_pips,
		initial_price,
	));
	assert_events_eq!(
		Runtime,
		RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::NewPoolCreated {
			unstable_asset,
			fee_hundredth_pips,
			initial_price,
		},)
	);
	System::reset_events();
}

fn new_account(account_id: &AccountId, role: AccountRole) {
	AccountRoles::on_new_account(account_id);
	assert_ok!(AccountRoles::register_account_role(account_id, role));
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
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, STABLE_ASSET).unwrap_or_default();
	assert_ok!(LiquidityPools::collect_and_mint_range_order(
		RuntimeOrigin::signed(account_id.clone()),
		unstable_asset,
		range,
		OldRangeOrderSize::Liquidity(liquidity),
	));
	let new_unstable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, unstable_asset).unwrap_or_default();
	let new_stable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, STABLE_ASSET).unwrap_or_default();

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
	check_balance(STABLE_ASSET, new_stable_balance, stable_balance);

	System::reset_events();
}

fn mint_limit_order(
	account_id: &AccountId,
	unstable_asset: Asset,
	order: Order,
	tick: Tick,
	amount: AssetAmount,
) {
	let unstable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, unstable_asset).unwrap_or_default();
	let stable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, STABLE_ASSET).unwrap_or_default();
	assert_ok!(LiquidityPools::collect_and_mint_limit_order(
		RuntimeOrigin::signed(account_id.clone()),
		unstable_asset,
		order,
		tick,
		amount,
	));
	let new_unstable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, unstable_asset).unwrap_or_default();
	let new_stable_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, STABLE_ASSET).unwrap_or_default();

	if order == Order::Sell {
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
	check_balance(STABLE_ASSET, new_stable_balance, stable_balance);

	System::reset_events();
}

fn setup_pool_and_accounts(assets: Vec<Asset>) {
	new_account(&DORIS, AccountRole::LiquidityProvider);
	new_account(&ZION, AccountRole::Broker);

	for asset in assets {
		new_pool(asset, 0u32, price_at_tick(0).unwrap());
		credit_account(&DORIS, asset, 1_000_000);
		credit_account(&DORIS, Asset::Usdc, 1_000_000);
		mint_range_order(&DORIS, asset, -1_000..1_000, 1_000_000);
	}
}

#[test]
fn basic_pool_setup_provision_and_swap() {
	super::genesis::default().build().execute_with(|| {
		new_pool(Asset::Eth, 0u32, price_at_tick(0).unwrap());
		new_pool(Asset::Flip, 0u32, price_at_tick(0).unwrap());

		new_account(&DORIS, AccountRole::LiquidityProvider);
		credit_account(&DORIS, Asset::Eth, 1_000_000);
		credit_account(&DORIS, Asset::Flip, 1_000_000);
		credit_account(&DORIS, Asset::Usdc, 1_000_000);

		mint_limit_order(&DORIS, Asset::Eth, Order::Sell, 0, 500_000);
		mint_range_order(&DORIS, Asset::Eth, -10..10, 1_000_000);

		mint_limit_order(&DORIS, Asset::Flip, Order::Sell, 0, 500_000);
		mint_range_order(&DORIS, Asset::Flip, -10..10, 1_000_000);

		new_account(&ZION, AccountRole::Broker);

		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ZION.clone()),
			Asset::Eth,
			Asset::Flip,
			EncodedAddress::Eth([1u8; 20]),
			0u16,
			None,
		));

		let deposit_address = <AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			cf_primitives::chains::assets::eth::Asset::Eth,
			pallet_cf_ingress_egress::ChannelIdCounter::<Runtime, EthereumInstance>::get(),
		).unwrap();

		let opened_at = EthereumChainTracking::get_block_height();

		assert_events_eq!(Runtime, RuntimeEvent::EthereumIngressEgress(
			pallet_cf_ingress_egress::Event::StartWitnessing { deposit_address, source_asset: cf_primitives::chains::assets::eth::Asset::Eth, opened_at },
		));
		System::reset_events();

		let current_epoch = Validator::current_epoch();
		for node in Validator::current_authorities() {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(node),
				Box::new(RuntimeCall::EthereumIngressEgress(pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address,
						asset: cf_primitives::chains::assets::eth::Asset::Eth,
						amount: 50,
						deposit_details: (),
					}],
					block_height: 0,
				})),
				current_epoch
			));
		}

		let swap_id = assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapScheduled {
			swap_id,
			deposit_amount: 50,
			origin: SwapOrigin::DepositChannel {
				deposit_address: events_deposit_address,
				..
			},
			..
		}) if <Ethereum as Chain>::ChainAccount::try_from(ChainAddressConverter::try_from_encoded_address(events_deposit_address.clone()).expect("we created the deposit address above so it should be valid")).unwrap() == deposit_address => swap_id);

		assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		state_chain_runtime::AllPalletsWithoutSystem::on_finalize(2);
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(3, Weight::from_parts(1_000_000_000_000, 0));

		let (.., egress_id) = assert_events_match!(
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
					..
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
fn can_process_ccm_via_swap_deposit_address() {
	super::genesis::default().build().execute_with(|| {
		// Setup pool and liquidity
		setup_pool_and_accounts(vec![Asset::Eth, Asset::Flip]);

		let gas_budget = 100;
		let deposit_amount = 1_000;
		let message = CcmChannelMetadata {
			message: vec![0u8, 1u8, 2u8, 3u8, 4u8],
			gas_budget,
			cf_parameters: vec![],
		};

		assert_ok!(Swapping::request_swap_deposit_address(
			RuntimeOrigin::signed(ZION.clone()),
			Asset::Flip,
			Asset::Usdc,
			EncodedAddress::Eth([0x02; 20]),
			0u16,
			Some(message),
		));

		// Deposit funds for the ccm.
		let deposit_address =
			<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
				cf_primitives::chains::assets::eth::Asset::Flip,
				pallet_cf_ingress_egress::ChannelIdCounter::<Runtime, EthereumInstance>::get(),
			)
			.unwrap();
		let current_epoch = Validator::current_epoch();
		for node in Validator::current_authorities() {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(node),
				Box::new(RuntimeCall::EthereumIngressEgress(
					pallet_cf_ingress_egress::Call::process_deposits {
						deposit_witnesses: vec![DepositWitness {
							deposit_address,
							asset: cf_primitives::chains::assets::eth::Asset::Flip,
							amount: 1_000,
							deposit_details: (),
						}],
						block_height: 0,
					}
				)),
				current_epoch
			));
		}
		let (principal_swap_id, gas_swap_id) = assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::CcmDepositReceived {
			ccm_id,
			principal_swap_id: Some(principal_swap_id),
			gas_swap_id: Some(gas_swap_id),
			deposit_amount: amount,
			..
		}) if ccm_id == CcmIdCounter::<Runtime>::get() &&
			amount == deposit_amount => (principal_swap_id, gas_swap_id));

		assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		state_chain_runtime::AllPalletsWithoutSystem::on_finalize(2);
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(3, Weight::from_parts(1_000_000_000_000, 0));

		let (.., egress_id) = assert_events_match!(
			Runtime,
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Flip,
					to: Asset::Usdc,
					input_amount: amount,
					..
				},
			) if amount == deposit_amount => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id,
					..
				},
			) if swap_id == principal_swap_id => (),
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
					..
				},
			) if swap_id == gas_swap_id => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::CcmEgressScheduled {
					ccm_id,
					egress_id: egress_id @ (ForeignChain::Ethereum, _),
				},
			) if ccm_id == CcmIdCounter::<Runtime>::get() => egress_id
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

#[test]
fn can_process_ccm_via_direct_deposit() {
	super::genesis::default().build().execute_with(|| {
		setup_pool_and_accounts(vec![Asset::Eth, Asset::Flip]);

		let gas_budget = 100;
		let deposit_amount = 1_000;
		let message = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![0u8, 1u8, 2u8, 3u8, 4u8],
				gas_budget,
				cf_parameters: vec![],
			},
		};

		let ccm_call = Box::new(RuntimeCall::Swapping(pallet_cf_swapping::Call::ccm_deposit{
			source_asset: Asset::Flip,
			deposit_amount,
			destination_asset: Asset::Usdc,
			destination_address: EncodedAddress::Eth([0x02; 20]),
			deposit_metadata: message,
			tx_hash: Default::default(),
		}));
		let current_epoch = Validator::current_epoch();
		for node in Validator::current_authorities() {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(node),
				ccm_call.clone(),
				current_epoch
			));
		}
		let (principal_swap_id, gas_swap_id) = assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::CcmDepositReceived {
			ccm_id,
			principal_swap_id: Some(principal_swap_id),
			gas_swap_id: Some(gas_swap_id),
			deposit_amount: amount,
			..
		}) if ccm_id == CcmIdCounter::<Runtime>::get() &&
			amount == deposit_amount => (principal_swap_id, gas_swap_id));

		assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		state_chain_runtime::AllPalletsWithoutSystem::on_finalize(2);
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(3, Weight::from_parts(1_000_000_000_000, 0));

		let (.., egress_id) = assert_events_match!(
			Runtime,
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Flip,
					to: Asset::Usdc,
					input_amount,
					..
				},
			) if input_amount == deposit_amount => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id,
					..
				},
			) if swap_id == principal_swap_id => (),
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
					..
				},
			) if swap_id == gas_swap_id => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::CcmEgressScheduled {
					ccm_id,
					egress_id: egress_id @ (ForeignChain::Ethereum, _),
				},
			) if ccm_id == CcmIdCounter::<Runtime>::get() => egress_id
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

#[test]
fn failed_swaps_are_rolled_back() {
	super::genesis::default().build().execute_with(|| {
		setup_pool_and_accounts(vec![Asset::Eth, Asset::Btc]);

		// Get current pool's liquidity
		let eth_price = LiquidityPools::current_price(Asset::Eth, STABLE_ASSET)
			.expect("Eth pool should be set up with liquidity.");
		let btc_price = LiquidityPools::current_price(Asset::Btc, STABLE_ASSET)
			.expect("Btc pool should be set up with liquidity.");

		let witness_swap_ingress =
			|from: Asset, to: Asset, amount: AssetAmount, destination_address: EncodedAddress| {
				let swap_call = Box::new(RuntimeCall::Swapping(
					pallet_cf_swapping::Call::schedule_swap_from_contract {
						from,
						to,
						deposit_amount: amount,
						destination_address,
						tx_hash: Default::default(),
					},
				));
				let current_epoch = Validator::current_epoch();
				for node in Validator::current_authorities() {
					assert_ok!(Witnesser::witness_at_epoch(
						RuntimeOrigin::signed(node),
						swap_call.clone(),
						current_epoch
					));
				}
			};

		witness_swap_ingress(
			Asset::Eth,
			Asset::Flip,
			1_000,
			EncodedAddress::Eth(Default::default()),
		);
		witness_swap_ingress(
			Asset::Eth,
			Asset::Btc,
			1_000,
			EncodedAddress::Btc("bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw".as_bytes().to_vec()),
		);
		witness_swap_ingress(
			Asset::Btc,
			Asset::Usdc,
			1_000,
			EncodedAddress::Eth(Default::default()),
		);
		System::reset_events();

		// Usdc -> Flip swap will fail. All swaps are stalled.
		Swapping::on_finalize(1);

		assert_events_match!(
			Runtime,
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::BatchSwapFailed {
					asset: Asset::Flip,
					direction: cf_primitives::SwapLeg::FromStable,
					amount: 998
				},
			) => ()
		);

		// Repeatedly processing Failed swaps should not impact pool liquidity
		assert_eq!(Some(eth_price), LiquidityPools::current_price(Asset::Eth, STABLE_ASSET));
		assert_eq!(Some(btc_price), LiquidityPools::current_price(Asset::Btc, STABLE_ASSET));

		// Subsequent swaps will also fail. No swaps should be processed and the Pool liquidity
		// shouldn't be drained.
		Swapping::on_finalize(2);
		assert_eq!(Some(eth_price), LiquidityPools::current_price(Asset::Eth, STABLE_ASSET));
		assert_eq!(Some(btc_price), LiquidityPools::current_price(Asset::Btc, STABLE_ASSET));

		// All swaps can continue once the problematic pool is fixed
		setup_pool_and_accounts(vec![Asset::Flip]);
		System::reset_events();

		Swapping::on_finalize(3);

		assert_ne!(Some(eth_price), LiquidityPools::current_price(Asset::Eth, STABLE_ASSET));
		assert_ne!(Some(btc_price), LiquidityPools::current_price(Asset::Btc, STABLE_ASSET));

		assert_events_match!(
			Runtime,
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset:: Eth,
					to: Asset::Usdc,
					input_amount: 2_000,
					..
				},
			) => (),
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset:: Btc,
					to: Asset::Usdc,
					input_amount: 1_000,
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
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Usdc,
					to: Asset::Btc,
					..
				},
			) => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id: 1,
					..
				},
			) => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id: 2,
					..
				},
			) => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapExecuted {
					swap_id: 3,
					..
				},
			) => ()
		);
	});
}
