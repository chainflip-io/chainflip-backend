//! Contains tests related to liquidity, pools and swapping
use std::vec;

use crate::{
	genesis,
	network::{
		fund_authorities_and_join_auction, new_account, register_refund_addresses,
		setup_account_and_peer_mapping, Cli, Network,
	},
	witness_call, witness_ethereum_rotation_broadcast, witness_rotation_broadcasts,
};
use cf_amm::{
	math::{price_at_tick, Price, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, EncodedAddress},
	assets::eth::Asset as EthAsset,
	eth::{api::EthereumApi, EthereumTrackedData},
	evm::DepositDetails,
	CcmChannelMetadata, CcmDepositMetadata, Chain, ChainState, ChannelRefundParameters,
	DefaultRetryPolicy, Ethereum, ExecutexSwapAndCall, ForeignChain, ForeignChainAddress,
	RetryPolicy, SwapOrigin, TransactionBuilder, TransferAssetParams,
};
use cf_primitives::{
	AccountId, AccountRole, Asset, AssetAmount, AuthorityCount, SwapId, FLIPPERINOS_PER_FLIP,
	GENESIS_EPOCH, STABLE_ASSET, SWAP_DELAY_BLOCKS,
};
use cf_test_utilities::{assert_events_eq, assert_events_match, assert_has_matching_event};
use cf_traits::{AdjustedFeeEstimationApi, AssetConverter, BalanceApi, EpochInfo, SwapType};
use frame_support::{
	assert_ok,
	instances::Instance1,
	traits::{OnFinalize, OnIdle, Time},
};
use pallet_cf_broadcast::{
	AwaitingBroadcast, BroadcastIdCounter, PendingApiCalls, RequestFailureCallbacks,
	RequestSuccessCallbacks,
};
use pallet_cf_ingress_egress::{DepositWitness, FailedForeignChainCall, VaultDepositWitness};
use pallet_cf_pools::{HistoricalEarnedFees, OrderId, RangeOrderSize};
use pallet_cf_swapping::{SwapRequestIdCounter, SwapRetryDelay};
use sp_core::{H160, U256};
use state_chain_runtime::{
	chainflip::{
		address_derivation::AddressDerivation, ChainAddressConverter, EthTransactionBuilder,
		EvmEnvironment,
	},
	AssetBalances, EthereumBroadcaster, EthereumChainTracking, EthereumIngressEgress,
	EthereumInstance, LiquidityPools, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, Swapping,
	System, Timestamp, Validator, Weight,
};

const DORIS: AccountId = AccountId::new([0x11; 32]);
const ZION: AccountId = AccountId::new([0x22; 32]);
const ETH_REFUND_PARAMS: ChannelRefundParameters<H160> = ChannelRefundParameters {
	retry_duration: 5,
	refund_address: sp_core::H160([100u8; 20]),
	min_price: sp_core::U256::zero(),
};

fn new_pool(unstable_asset: Asset, fee_hundredth_pips: u32, initial_price: Price) {
	assert_ok!(LiquidityPools::new_pool(
		pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
		unstable_asset,
		STABLE_ASSET,
		fee_hundredth_pips,
		initial_price,
	));
	assert_events_eq!(
		Runtime,
		RuntimeEvent::LiquidityPools(pallet_cf_pools::Event::NewPoolCreated {
			base_asset: unstable_asset,
			quote_asset: STABLE_ASSET,
			fee_hundredth_pips,
			initial_price,
		},)
	);
	System::reset_events();
}

fn credit_account(account_id: &AccountId, asset: Asset, amount: AssetAmount) {
	let original_amount = pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, asset);
	AssetBalances::credit_account(account_id, asset, amount);
	assert_eq!(
		pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, asset),
		original_amount + amount
	);
	assert_has_matching_event!(
		Runtime,
		RuntimeEvent::AssetBalances(pallet_cf_asset_balances::Event::AccountCredited {
			account_id: event_account_id,
			asset: event_asset,
			amount_credited,
			..
		}) if *amount_credited == amount && event_account_id == account_id && *event_asset == asset
	);
	System::reset_events();
}

#[track_caller]
fn set_range_order(
	account_id: &AccountId,
	base_asset: Asset,
	quote_asset: Asset,
	id: OrderId,
	range: Option<core::ops::Range<Tick>>,
	liquidity: Liquidity,
) {
	let balances = [base_asset, quote_asset]
		.map(|asset| pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, asset));
	assert_ok!(LiquidityPools::set_range_order(
		RuntimeOrigin::signed(account_id.clone()),
		base_asset,
		quote_asset,
		id,
		range,
		RangeOrderSize::Liquidity { liquidity },
	));
	let new_balances = [base_asset, quote_asset]
		.map(|asset| pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, asset));

	assert!(new_balances.into_iter().zip(balances).all(|(new, old)| { new <= old }));

	for ((new_balance, old_balance), expected_asset) in
		new_balances.into_iter().zip(balances).zip([base_asset, quote_asset])
	{
		if new_balance < old_balance {
			assert_has_matching_event!(
				Runtime,
				RuntimeEvent::AssetBalances(pallet_cf_asset_balances::Event::AccountDebited {
					account_id: event_account_id,
					asset,
					amount_debited,
					..
				}) if event_account_id == account_id && *asset == expected_asset && *amount_debited == old_balance - new_balance
			);
		}
	}

	System::reset_events();
}

fn set_limit_order(
	account_id: &AccountId,
	sell_asset: Asset,
	buy_asset: Asset,
	id: OrderId,
	tick: Option<Tick>,
	sell_amount: AssetAmount,
) {
	let (asset_pair, order) = pallet_cf_pools::AssetPair::from_swap(sell_asset, buy_asset).unwrap();

	let sell_balance =
		pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, sell_asset);
	let buy_balance = pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, buy_asset);
	assert_ok!(LiquidityPools::set_limit_order(
		RuntimeOrigin::signed(account_id.clone()),
		asset_pair.assets().base,
		asset_pair.assets().quote,
		order,
		id,
		tick,
		sell_amount,
	));
	let new_sell_balance =
		pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, sell_asset);
	let new_buy_balance =
		pallet_cf_asset_balances::FreeBalances::<Runtime>::get(account_id, buy_asset);

	assert_eq!(new_sell_balance, sell_balance - sell_amount);
	assert_eq!(new_buy_balance, buy_balance);

	if new_sell_balance < sell_balance {
		assert_has_matching_event!(
			Runtime,
			RuntimeEvent::AssetBalances(pallet_cf_asset_balances::Event::AccountDebited {
				account_id: event_account_id,
				asset,
				amount_debited,
				..
			}) if event_account_id == account_id && *asset == sell_asset && *amount_debited == sell_balance - new_sell_balance
		);
	}

	System::reset_events();
}

#[derive(Clone, Copy)]
pub enum OrderType {
	LimitOrder,
	RangeOrder,
}

pub fn add_liquidity(
	asset: Asset,
	amount: AssetAmount,
	order_type: OrderType,
	order_id: Option<u64>,
) {
	use rand::Rng;
	// We use random order id to make collisions with any existing orders near impossible:
	let order_id: u64 = order_id.unwrap_or_else(|| rand::thread_rng().gen());

	assert!(LiquidityPools::pool_info(asset, Asset::Usdc).is_ok(), "pool must be set up first");

	credit_account(&DORIS, asset, amount);
	credit_account(&DORIS, Asset::Usdc, amount);

	match order_type {
		OrderType::LimitOrder => {
			set_limit_order(&DORIS, asset, Asset::Usdc, order_id, Some(0), amount);
			set_limit_order(&DORIS, Asset::Usdc, asset, u64::MAX - order_id, Some(0), amount);
		},
		OrderType::RangeOrder => {
			set_range_order(&DORIS, asset, Asset::Usdc, order_id, Some(-10..10), amount);
		},
	}
}

#[track_caller]
pub fn setup_pool_and_accounts(assets: Vec<Asset>, order_type: OrderType) {
	new_account(&DORIS, AccountRole::LiquidityProvider);
	new_account(&ZION, AccountRole::Broker);

	const DECIMALS: u128 = 10u128.pow(18);

	for (order_id, asset) in (0..).zip(assets) {
		new_pool(asset, 0u32, price_at_tick(0).unwrap());
		add_liquidity(asset, 10_000_000 * DECIMALS, order_type, Some(order_id));
	}
}

#[test]
fn basic_pool_setup_provision_and_swap() {
	super::genesis::with_test_defaults()
		.with_additional_accounts(&[
			(DORIS, AccountRole::LiquidityProvider, 5 * FLIPPERINOS_PER_FLIP),
			(ZION, AccountRole::Broker, 5 * FLIPPERINOS_PER_FLIP),
		])
		.build()
		.execute_with(|| {
			new_pool(Asset::Eth, 0, price_at_tick(0).unwrap());
			new_pool(Asset::Flip, 0, price_at_tick(0).unwrap());
			register_refund_addresses(&DORIS);

			// Use the same decimals amount for all assets.
			const DECIMALS: u128 = 10u128.pow(18);
			credit_account(&DORIS, Asset::Eth, 10_000_000 * DECIMALS);
			credit_account(&DORIS, Asset::Flip, 10_000_000 * DECIMALS);
			credit_account(&DORIS, Asset::Usdc, 10_000_000 * DECIMALS);
			assert!(!HistoricalEarnedFees::<Runtime>::contains_key(&DORIS, Asset::Eth));
			assert!(!HistoricalEarnedFees::<Runtime>::contains_key(&DORIS, Asset::Flip));
			assert!(!HistoricalEarnedFees::<Runtime>::contains_key(&DORIS, Asset::Usdc));

			set_limit_order(&DORIS, Asset::Eth, Asset::Usdc, 0, Some(0), 1_000_000 * DECIMALS);
			set_limit_order(&DORIS, Asset::Usdc, Asset::Eth, 0, Some(0), 1_000_000 * DECIMALS);
			set_limit_order(&DORIS, Asset::Flip, Asset::Usdc, 0, Some(0), 1_000_000 * DECIMALS);
			set_limit_order(&DORIS, Asset::Usdc, Asset::Flip, 0, Some(0), 1_000_000 * DECIMALS);

			assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
				RuntimeOrigin::signed(ZION.clone()),
				Asset::Eth,
				Asset::Flip,
				EncodedAddress::Eth([1u8; 20]),
				0,
				None,
				0u16,
				Default::default(),
				None,
				None,
			));

			let deposit_address =
				<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
					EthAsset::Eth,
					pallet_cf_ingress_egress::ChannelIdCounter::<Runtime, EthereumInstance>::get(),
				)
				.unwrap();

			System::reset_events();
			const DEPOSIT_AMOUNT: u128 = 5_000 * DECIMALS;
			witness_call(RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address,
						asset: EthAsset::Eth,
						amount: (DEPOSIT_AMOUNT + EthereumChainTracking::estimate_ingress_fee(EthAsset::Eth)),
						deposit_details: Default::default(),
					}],
					block_height: 0,
				},
			));

			let swap_request_id = assert_events_match!(Runtime,
			RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequested {
			    swap_request_id,
				input_amount: DEPOSIT_AMOUNT,
				origin: SwapOrigin::DepositChannel {
					deposit_address: events_deposit_address,
					..
				},
				..
			}) if <Ethereum as
			Chain>::ChainAccount::try_from(ChainAddressConverter::try_from_encoded_address(events_deposit_address.
			clone()).expect("we created the deposit address above so it should be
			valid")).unwrap() == deposit_address => swap_request_id);

			assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
			state_chain_runtime::AllPalletsWithoutSystem::on_finalize(2);
			state_chain_runtime::AllPalletsWithoutSystem::on_idle(
				3,
				Weight::from_parts(1_000_000_000_000, 0),
			);
			assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
			state_chain_runtime::AllPalletsWithoutSystem::on_finalize(3);
			state_chain_runtime::AllPalletsWithoutSystem::on_idle(
				4,
				Weight::from_parts(1_000_000_000_000, 0),
			);

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
						swap_request_id: executed_swap_request_id,
						..
					},
				) if executed_swap_request_id == swap_request_id => (),
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

			assert!(HistoricalEarnedFees::<Runtime>::contains_key(&DORIS, Asset::Eth));
			assert!(HistoricalEarnedFees::<Runtime>::contains_key(&DORIS, Asset::Flip));
			assert!(HistoricalEarnedFees::<Runtime>::contains_key(&DORIS, Asset::Usdc));
		});
}

#[test]
fn can_process_ccm_via_swap_deposit_address() {
	const DECIMALS: u128 = 10u128.pow(18);
	const GAS_BUDGET: AssetAmount = 50 * DECIMALS;
	const DEPOSIT_AMOUNT: AssetAmount = 50_000 * DECIMALS;

	super::genesis::with_test_defaults().build().execute_with(|| {
		// Setup pool and liquidity
		setup_pool_and_accounts(vec![Asset::Eth, Asset::Flip], OrderType::LimitOrder);

		let message = CcmChannelMetadata {
			message: vec![0u8, 1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
			gas_budget: GAS_BUDGET,
			ccm_additional_data: Default::default(),
		};

		assert_ok!(Swapping::request_swap_deposit_address_with_affiliates(
			RuntimeOrigin::signed(ZION.clone()),
			Asset::Flip,
			Asset::Usdc,
			EncodedAddress::Eth([0x02; 20]),
			0,
			Some(message),
			0u16,
			Default::default(),
			None,
			None,
		));

		// Deposit funds for the ccm.
		let deposit_address =
			<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
				EthAsset::Flip,
				pallet_cf_ingress_egress::ChannelIdCounter::<Runtime, EthereumInstance>::get(),
			)
			.unwrap();
		let ingress_fee = sp_std::cmp::min(
			Swapping::calculate_input_for_gas_output::<Ethereum>(
				EthAsset::Flip,
				EthereumChainTracking::estimate_ingress_fee(EthAsset::Flip),
			)
			.unwrap(),
			u128::MAX,
		);
		witness_call(RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::process_deposits {
				deposit_witnesses: vec![DepositWitness {
					deposit_address,
					asset: EthAsset::Flip,
					amount: (DEPOSIT_AMOUNT + ingress_fee),
					deposit_details: Default::default(),
				}],
				block_height: 0,
			},
		));

		let swap_request_id = assert_events_match!(
		Runtime,
		RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequested {
			swap_request_id,
			input_amount: DEPOSIT_AMOUNT,
			..
		}) if swap_request_id == SwapRequestIdCounter::<Runtime>::get() => swap_request_id
		);

		assert_events_match!(
		Runtime,
		RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapScheduled {
			swap_request_id: swap_request_id_in_event,
			swap_id,
			input_amount: DEPOSIT_AMOUNT,
			swap_type: SwapType::Swap,
			execute_at: 3,
			..
		}) if swap_request_id == SwapRequestIdCounter::<Runtime>::get() &&
			swap_request_id_in_event == swap_request_id => swap_id
		);

		assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		state_chain_runtime::AllPalletsWithoutSystem::on_finalize(2);
		System::set_block_number(3);
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			3,
			Weight::from_parts(1_000_000_000_000, 0),
		);

		assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		state_chain_runtime::AllPalletsWithoutSystem::on_finalize(3);
		System::set_block_number(4);
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			4,
			Weight::from_parts(1_000_000_000_000, 0),
		);

		assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		state_chain_runtime::AllPalletsWithoutSystem::on_finalize(4);
		System::set_block_number(5);
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			5,
			Weight::from_parts(1_000_000_000_000, 0),
		);

		assert_ok!(Timestamp::set(RuntimeOrigin::none(), Timestamp::now()));
		state_chain_runtime::AllPalletsWithoutSystem::on_finalize(5);
		System::set_block_number(6);
		state_chain_runtime::AllPalletsWithoutSystem::on_idle(
			6,
			Weight::from_parts(1_000_000_000_000, 0),
		);

		let (.., egress_id) = assert_events_match!(
			Runtime,
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset::Usdc,
					to: Asset::Eth,
					..
				},
			) => (),
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::SwapEgressScheduled {
					swap_request_id: swap_request_id_in_event,
					egress_id: egress_id @ (ForeignChain::Ethereum, _),
					..
				},
			) if swap_request_id_in_event == swap_request_id => egress_id
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

fn ccm_deposit_metadata_mock() -> CcmDepositMetadata {
	CcmDepositMetadata {
		source_chain: ForeignChain::Ethereum,
		source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
		channel_metadata: CcmChannelMetadata {
			message: vec![0u8, 1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
			gas_budget: 100_000_000,
			ccm_additional_data: Default::default(),
		},
	}
}

fn vault_swap_deposit_witness(
	deposit_amount: u128,
	output_asset: Asset,
) -> VaultDepositWitness<Runtime, EthereumInstance> {
	VaultDepositWitness {
		input_asset: EthAsset::Eth,
		output_asset,
		deposit_amount,
		destination_address: EncodedAddress::Eth([0x02; 20]),
		deposit_metadata: Some(ccm_deposit_metadata_mock()),
		tx_id: Default::default(),
		deposit_details: DepositDetails { tx_hashes: None },
		broker_fee: None,
		affiliate_fees: Default::default(),
		refund_params: Some(ETH_REFUND_PARAMS),
		dca_params: None,
		boost_fee: 0,
		deposit_address: Some(H160::from([0x03; 20])),
		channel_id: Some(0),
	}
}

#[test]
fn can_process_ccm_via_direct_deposit() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		setup_pool_and_accounts(vec![Asset::Eth, Asset::Flip], OrderType::LimitOrder);

		let deposit_amount = 100_000_000_000;

		witness_call(RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::vault_swap_request {
				block_height: 0,
				deposit: Box::new(vault_swap_deposit_witness(deposit_amount, Asset::Usdc)),
			},
		));

		// It is sufficient to check that swap is "requested", the rest is
		// covered by the `can_process_ccm_via_swap_deposit_address` test
		assert_events_match!(Runtime, RuntimeEvent::Swapping(pallet_cf_swapping::Event::SwapRequested {
				swap_request_id,
				input_amount: amount,
				..
			}) if swap_request_id == SwapRequestIdCounter::<Runtime>::get() &&
				amount == deposit_amount => ()
		);
	});
}

#[test]
fn failed_swaps_are_rolled_back() {
	let get_pool = |asset| {
		pallet_cf_pools::pallet::Pools::<Runtime>::get(
			pallet_cf_pools::AssetPair::new(asset, Asset::Usdc).expect("invalid asset pair"),
		)
		.expect("pool must exist")
	};

	const DECIMALS: u128 = 10u128.pow(18);

	super::genesis::with_test_defaults().build().execute_with(|| {
		setup_pool_and_accounts(vec![Asset::Eth, Asset::Btc, Asset::Flip], OrderType::RangeOrder);

		// Give ETH pool extra liquidity to ensure it is not the reason the incoming
		// swap will fail:
		add_liquidity(Asset::Eth, 10_000_000 * DECIMALS, OrderType::RangeOrder, None);

		// Get the current state of pools so we can compare agaist this later:
		let eth_pool = get_pool(Asset::Eth);
		let flip_pool = get_pool(Asset::Flip);

		witness_call(RuntimeCall::EthereumIngressEgress(
			pallet_cf_ingress_egress::Call::vault_swap_request {
				block_height: 0,
				deposit: Box::new(vault_swap_deposit_witness(10_000 * DECIMALS, Asset::Flip)),
			},
		));

		System::reset_events();

		let swaps_scheduled_at = System::block_number() + SWAP_DELAY_BLOCKS;

		Swapping::on_finalize(swaps_scheduled_at);
		// FLIP pool does not have enough liquidity, so USDC->FLIP leg will fail,
		// and any changes to the ETH pool should be reverted:

		assert_events_match!(
			Runtime,
			RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::BatchSwapFailed {
					asset: Asset::Flip,
					direction: cf_primitives::SwapLeg::FromStable,
					..
				},
			) => ()
		);

		// State of pools has not changed:
		assert_eq!(eth_pool, get_pool(Asset::Eth));
		assert_eq!(flip_pool, get_pool(Asset::Flip));

		// After FLIP liquidity is added, the swap should go through:
		add_liquidity(Asset::Flip, 10_000_000 * DECIMALS, OrderType::RangeOrder, None);

		System::reset_events();

		Swapping::on_finalize(swaps_scheduled_at + SwapRetryDelay::<Runtime>::get());

		// Now the state of pools has changed (sanity check):
		assert_ne!(eth_pool, get_pool(Asset::Eth));
		assert_ne!(flip_pool, get_pool(Asset::Flip));

		assert_events_match!(
			Runtime,
			RuntimeEvent::LiquidityPools(
				pallet_cf_pools::Event::AssetSwapped {
					from: Asset:: Eth,
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
					swap_id: SwapId(1),
					..
				},
			) => ()
		);
	});
}

#[test]
fn ethereum_ccm_can_calculate_gas_limits() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let chain_state = ChainState::<Ethereum> {
			block_height: 1,
			tracked_data: EthereumTrackedData {
				base_fee: 1_000_000u128,
				priority_fee: 500_000u128,
			},
		};

		witness_call(RuntimeCall::EthereumChainTracking(
			pallet_cf_chain_tracking::Call::update_chain_state {
				new_chain_state: chain_state.clone(),
			},
		));
		assert_eq!(EthereumChainTracking::chain_state(), Some(chain_state));

		let make_ccm_call = |gas_budget: u128| {
			<EthereumApi<EvmEnvironment> as ExecutexSwapAndCall<Ethereum>>::new_unsigned(
				TransferAssetParams::<Ethereum> {
					asset: EthAsset::Flip,
					amount: 1_000,
					to: Default::default(),
				},
				ForeignChain::Ethereum,
				None,
				gas_budget,
				vec![],
				vec![],
				Default::default(),
			)
			.unwrap()
		};

		assert_eq!(
			EthTransactionBuilder::calculate_gas_limit(&make_ccm_call(1_499_999)),
			Some(U256::from(1_499_999) + U256::from(120_000))
		);
		assert_eq!(
			EthTransactionBuilder::calculate_gas_limit(&make_ccm_call(1_500_000)),
			Some(U256::from(1_500_000) + U256::from(120_000))
		);
	});
}

#[test]
fn can_resign_failed_ccm() {
	const EPOCH_DURATION_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_DURATION_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			// Setup environments, and rotate into the next epoch.
			let (mut testnet, _genesis, _backup_nodes) =
				fund_authorities_and_join_auction(MAX_AUTHORITIES);

			testnet.move_to_the_next_epoch();

			witness_ethereum_rotation_broadcast(1);
			setup_pool_and_accounts(vec![Asset::Eth, Asset::Flip], OrderType::LimitOrder);

			witness_call(RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::vault_swap_request {
					block_height: 0,
					deposit: Box::new(vault_swap_deposit_witness(10_000_000_000_000, Asset::Usdc)),
				},
			));

			// Process the swap -> egress -> threshold sign -> broadcast
			let starting_epoch = Validator::current_epoch();
			testnet.move_forward_blocks(5);
			let broadcast_id = BroadcastIdCounter::<Runtime, Instance1>::get();

			// Fail the broadcast
			for _ in Validator::current_authorities() {
				let nominee = AwaitingBroadcast::<Runtime, Instance1>::get(broadcast_id)
					.unwrap_or_else(|| {
						panic!(
							"Failed to get the transaction signing attempt for {:?}.",
							broadcast_id,
						)
					})
					.nominee
					.unwrap();

				assert_ok!(EthereumBroadcaster::transaction_failed(
					RuntimeOrigin::signed(nominee),
					broadcast_id,
				));
				testnet.move_forward_blocks(
					DefaultRetryPolicy::next_attempt_delay(EthereumBroadcaster::attempt_count(
						broadcast_id,
					))
					.unwrap(),
				);
			}

			// Upon broadcast failure, the Failure callback is called, and failed CCM is stored.
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(broadcast_id),
				vec![FailedForeignChainCall { broadcast_id: 2, original_epoch: 2 }]
			);

			// No storage change within the same epoch
			testnet.move_to_the_end_of_epoch();
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch),
				vec![FailedForeignChainCall { broadcast_id: 2, original_epoch: 2 }]
			);

			// On the next epoch, the call is asked to be resigned
			testnet.move_to_the_next_epoch();
			// the rotation tx for ethereum is the third broadcast overall (2 broadcasts already
			// created above) whereas for other chains it is the first broadcast
			witness_rotation_broadcasts([3, 1, 1, 1, 1]);
			testnet.move_forward_blocks(2);

			assert_eq!(EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch), vec![]);
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch + 1),
				vec![FailedForeignChainCall { broadcast_id: 2, original_epoch: 2 }]
			);

			// On the next epoch, the failed call is removed from storage.
			testnet.move_to_the_next_epoch();
			// the rotation tx for ethereum is the fourth broadcast overall (3 broadcasts already
			// created above) whereas for other chains it is the second broadcast (first broadcast
			// was the previous rotation)
			witness_rotation_broadcasts([4, 2, 2, 2, 2]);
			testnet.move_forward_blocks(2);
			assert_eq!(EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch), vec![]);
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch + 1),
				vec![]
			);
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch + 2),
				vec![]
			);

			assert!(PendingApiCalls::<Runtime, Instance1>::get(broadcast_id).is_none());
			assert!(RequestFailureCallbacks::<Runtime, Instance1>::get(broadcast_id).is_none());
			assert!(RequestSuccessCallbacks::<Runtime, Instance1>::get(broadcast_id).is_none());
		});
}

#[test]
fn can_handle_failed_vault_transfer() {
	const EPOCH_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::with_test_defaults()
		.epoch_duration(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			// Setup environments, and rotate into the next epoch.
			let (mut testnet, backup_nodes) =
				Network::create(10, &Validator::current_authorities());
			for node in &backup_nodes {
				testnet.state_chain_gateway_contract.fund_account(
					node.clone(),
					genesis::GENESIS_BALANCE,
					GENESIS_EPOCH,
				);
			}
			testnet.move_forward_blocks(1);
			for node in backup_nodes.clone() {
				Cli::register_as_validator(&node);
				setup_account_and_peer_mapping(&node);
				Cli::start_bidding(&node);
			}

			testnet.move_to_the_next_epoch();
			witness_ethereum_rotation_broadcast(1);

			// Report a failed vault transfer
			let starting_epoch = Validator::current_epoch();
			let asset = EthAsset::Eth;
			let amount = 1_000_000u128;
			let destination_address = [0x00; 20].into();
			let broadcast_id = 2;

			witness_call(RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::vault_transfer_failed {
					asset,
					amount,
					destination_address,
				},
			));

			System::assert_has_event(RuntimeEvent::EthereumIngressEgress(
				pallet_cf_ingress_egress::Event::<Runtime, Instance1>::TransferFallbackRequested {
					asset,
					amount,
					destination_address,
					broadcast_id,
					egress_details: None,
				},
			));
			testnet.move_forward_blocks(11);

			// Transfer Fallback call is constructed, but not broadcasted.
			assert!(EthereumBroadcaster::threshold_signature_data(broadcast_id).is_some());
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch),
				vec![FailedForeignChainCall { broadcast_id, original_epoch: starting_epoch }]
			);

			// No storage change within the same epoch
			testnet.move_to_the_end_of_epoch();
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch),
				vec![FailedForeignChainCall { broadcast_id, original_epoch: starting_epoch }]
			);

			// On the next epoch, the call is asked to be resigned
			testnet.move_to_the_next_epoch();
			// the rotation tx for ethereum is the third broadcast (2 broadcasts already created
			// above) whereas for other chains it is the first broadcast
			witness_rotation_broadcasts([3, 1, 1, 1, 1]);
			testnet.move_forward_blocks(2);

			assert_eq!(EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch), vec![]);
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch + 1),
				vec![FailedForeignChainCall { broadcast_id, original_epoch: starting_epoch }]
			);

			// On the next epoch, the failed call is removed from storage.
			testnet.move_to_the_next_epoch();
			testnet.move_forward_blocks(2);
			assert_eq!(EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch), vec![]);
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch + 1),
				vec![]
			);
			assert_eq!(
				EthereumIngressEgress::failed_foreign_chain_calls(starting_epoch + 2),
				vec![]
			);

			assert!(PendingApiCalls::<Runtime, Instance1>::get(broadcast_id).is_none());
			assert!(RequestFailureCallbacks::<Runtime, Instance1>::get(broadcast_id).is_none());
			assert!(RequestSuccessCallbacks::<Runtime, Instance1>::get(broadcast_id).is_none());
		});
}
