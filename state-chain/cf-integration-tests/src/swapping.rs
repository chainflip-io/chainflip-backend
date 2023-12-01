//! Contains tests related to liquidity, pools and swapping
use crate::{
	genesis,
	network::{setup_account_and_peer_mapping, Cli, Network},
};
use cf_amm::{
	common::{price_at_tick, Price, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, EncodedAddress},
	assets::eth::Asset as EthAsset,
	eth::{api::EthereumApi, EthereumTrackedData},
	CcmChannelMetadata, CcmDepositMetadata, Chain, ChainState, Ethereum, ExecutexSwapAndCall,
	ForeignChain, ForeignChainAddress, SwapOrigin, TransactionBuilder, TransferAssetParams,
};
use cf_primitives::{
	AccountId, AccountRole, Asset, AssetAmount, AuthorityCount, GENESIS_EPOCH, STABLE_ASSET,
};
use cf_test_utilities::{assert_events_eq, assert_events_match};
use cf_traits::{AccountRoleRegistry, EpochInfo, LpBalanceApi};
use frame_support::{
	assert_ok,
	instances::Instance1,
	traits::{OnFinalize, OnIdle, OnNewAccount},
};
use pallet_cf_broadcast::{
	AwaitingBroadcast, BroadcastAttemptId, RequestFailureCallbacks, RequestSuccessCallbacks,
	ThresholdSignatureData, TransactionSigningAttempt,
};
use pallet_cf_ingress_egress::{DepositWitness, FailedCcm};
use pallet_cf_pools::{OrderId, RangeOrderSize};
use pallet_cf_swapping::CcmIdCounter;
use sp_core::U256;
use state_chain_runtime::{
	chainflip::{
		address_derivation::AddressDerivation, ChainAddressConverter, EthEnvironment,
		EthTransactionBuilder,
	},
	AccountRoles, EthereumBroadcaster, EthereumChainTracking, EthereumIngressEgress,
	EthereumInstance, LiquidityPools, LiquidityProvider, Runtime, RuntimeCall, RuntimeEvent,
	RuntimeOrigin, Swapping, System, Timestamp, Validator, Weight, Witnesser,
};

const DORIS: AccountId = AccountId::new([0x11; 32]);
const ZION: AccountId = AccountId::new([0x22; 32]);

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
			pair_asset: STABLE_ASSET,
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

fn register_refund_addressses(account_id: &AccountId) {
	for encoded_address in [
		EncodedAddress::Eth(Default::default()),
		EncodedAddress::Dot(Default::default()),
		EncodedAddress::Btc("bcrt1qs758ursh4q9z627kt3pp5yysm78ddny6txaqgw".as_bytes().to_vec()),
	] {
		assert_ok!(LiquidityProvider::register_liquidity_refund_address(
			RuntimeOrigin::signed(account_id.clone()),
			encoded_address
		));
	}
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

fn set_range_order(
	account_id: &AccountId,
	base_asset: Asset,
	pair_asset: Asset,
	id: OrderId,
	range: Option<core::ops::Range<Tick>>,
	liquidity: Liquidity,
) {
	let balances = [base_asset, pair_asset].map(|asset| {
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, asset).unwrap_or_default()
	});
	assert_ok!(LiquidityPools::set_range_order(
		RuntimeOrigin::signed(account_id.clone()),
		base_asset,
		pair_asset,
		id,
		range,
		RangeOrderSize::Liquidity { liquidity },
	));
	let new_balances = [base_asset, pair_asset].map(|asset| {
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, asset).unwrap_or_default()
	});

	assert!(new_balances.into_iter().zip(balances).all(|(new, old)| { new <= old }));

	for ((new_balance, old_balance), asset) in
		new_balances.into_iter().zip(balances).zip([base_asset, pair_asset])
	{
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
	let sell_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, sell_asset).unwrap_or_default();
	let buy_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, buy_asset).unwrap_or_default();
	assert_ok!(LiquidityPools::set_limit_order(
		RuntimeOrigin::signed(account_id.clone()),
		sell_asset,
		buy_asset,
		id,
		tick,
		sell_amount,
	));
	let new_sell_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, sell_asset).unwrap_or_default();
	let new_buy_balance =
		pallet_cf_lp::FreeBalances::<Runtime>::get(account_id, buy_asset).unwrap_or_default();

	assert_eq!(new_sell_balance, sell_balance - sell_amount);
	assert_eq!(new_buy_balance, buy_balance);

	if new_sell_balance < sell_balance {
		assert_events_eq!(
			Runtime,
			RuntimeEvent::LiquidityProvider(pallet_cf_lp::Event::AccountDebited {
				account_id: account_id.clone(),
				asset: sell_asset,
				amount_debited: sell_balance - new_sell_balance,
			},)
		);
	}

	System::reset_events();
}

fn setup_pool_and_accounts(assets: Vec<Asset>) {
	new_account(&DORIS, AccountRole::LiquidityProvider);
	register_refund_addressses(&DORIS);

	new_account(&ZION, AccountRole::Broker);

	for asset in assets {
		new_pool(asset, 0u32, price_at_tick(0).unwrap());
		credit_account(&DORIS, asset, 1_000_000);
		credit_account(&DORIS, Asset::Usdc, 1_000_000);
		set_range_order(&DORIS, asset, Asset::Usdc, 0, Some(-1_000..1_000), 1_000_000);
	}
}

#[test]
fn basic_pool_setup_provision_and_swap() {
	super::genesis::default().build().execute_with(|| {
		new_pool(Asset::Eth, 0u32, price_at_tick(0).unwrap());
		new_pool(Asset::Flip, 0u32, price_at_tick(0).unwrap());

		new_account(&DORIS, AccountRole::LiquidityProvider);
		register_refund_addressses(&DORIS);
		credit_account(&DORIS, Asset::Eth, 1_000_000);
		credit_account(&DORIS, Asset::Flip, 1_000_000);
		credit_account(&DORIS, Asset::Usdc, 1_000_000);

		set_limit_order(&DORIS, Asset::Eth, Asset::Usdc, 0, Some(0), 500_000);
		set_range_order(&DORIS, Asset::Eth, Asset::Usdc, 0, Some(-10..10), 1_000_000);

		set_limit_order(&DORIS, Asset::Flip, Asset::Usdc, 0, Some(0), 500_000);
		set_range_order(&DORIS, Asset::Flip, Asset::Usdc, 0, Some(-10..10), 1_000_000);

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
			message: vec![0u8, 1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
			gas_budget,
			cf_parameters: Default::default(),
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
		let deposit_metadata = CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![0u8, 1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
				gas_budget,
				cf_parameters: Default::default(),
			},
		};

		let ccm_call = Box::new(RuntimeCall::Swapping(pallet_cf_swapping::Call::ccm_deposit{
			source_asset: Asset::Flip,
			deposit_amount,
			destination_asset: Asset::Usdc,
			destination_address: EncodedAddress::Eth([0x02; 20]),
			deposit_metadata,
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
					..
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

#[test]
fn ethereum_ccm_can_calculate_gas_limits() {
	super::genesis::default().build().execute_with(|| {
		let current_epoch = Validator::current_epoch();
		let chain_state = ChainState::<Ethereum> {
			block_height: 1,
			tracked_data: EthereumTrackedData {
				base_fee: 1_000_000u128,
				priority_fee: 500_000u128,
			},
		};

		for node in Validator::current_authorities() {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(node),
				Box::new(RuntimeCall::EthereumChainTracking(
					pallet_cf_chain_tracking::Call::update_chain_state {
						new_chain_state: chain_state.clone(),
					}
				)),
				current_epoch
			));
		}
		assert_eq!(EthereumChainTracking::chain_state(), Some(chain_state));

		let make_ccm_call = |gas_budget: u128| {
			<EthereumApi<EthEnvironment> as ExecutexSwapAndCall<Ethereum>>::new_unsigned(
				TransferAssetParams::<Ethereum> {
					asset: EthAsset::Flip,
					amount: 1_000,
					to: Default::default(),
				},
				ForeignChain::Ethereum,
				None,
				gas_budget,
				vec![],
			)
			.unwrap()
		};

		// Each unit of gas costs 1 * 1_000_000 + 500_000 = 1_500_000
		assert_eq!(
			EthTransactionBuilder::calculate_gas_limit(&make_ccm_call(1_499_999)),
			Some(U256::from(0))
		);
		assert_eq!(
			EthTransactionBuilder::calculate_gas_limit(&make_ccm_call(1_500_000)),
			Some(U256::from(1))
		);
		// 1_000_000_000_000 / (1 * 1_000_000 + 500_000) = 666_666
		assert_eq!(
			EthTransactionBuilder::calculate_gas_limit(&make_ccm_call(1_000_000_000_000u128)),
			Some(U256::from(666_666))
		);

		// Can handle divide by zero case. Practically this should never happen.
		let chain_state = ChainState::<Ethereum> {
			block_height: 2,
			tracked_data: EthereumTrackedData { base_fee: 0u128, priority_fee: 0u128 },
		};

		for node in Validator::current_authorities() {
			assert_ok!(Witnesser::witness_at_epoch(
				RuntimeOrigin::signed(node),
				Box::new(RuntimeCall::EthereumChainTracking(
					pallet_cf_chain_tracking::Call::update_chain_state {
						new_chain_state: chain_state.clone(),
					}
				)),
				current_epoch
			));
		}

		assert_eq!(
			EthTransactionBuilder::calculate_gas_limit(&make_ccm_call(1_000_000_000u128)),
			Some(U256::from(0))
		);
	});
}

#[test]
fn can_resign_failed_ccm() {
	const EPOCH_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 10;
	super::genesis::default()
		.blocks_per_epoch(EPOCH_BLOCKS)
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
			setup_pool_and_accounts(vec![Asset::Eth, Asset::Flip]);

			// Deposit CCM and process the swap
			let deposit_metadata = CcmDepositMetadata {
				source_chain: ForeignChain::Ethereum,
				source_address: Some(ForeignChainAddress::Eth([0xcf; 20].into())),
				channel_metadata: CcmChannelMetadata {
					message: vec![0u8, 1u8, 2u8, 3u8, 4u8].try_into().unwrap(),
					gas_budget: 1_000,
					cf_parameters: Default::default(),
				},
			};

			let ccm_call = Box::new(RuntimeCall::Swapping(pallet_cf_swapping::Call::ccm_deposit {
				source_asset: Asset::Flip,
				deposit_amount: 10_000,
				destination_asset: Asset::Usdc,
				destination_address: EncodedAddress::Eth([0x02; 20]),
				deposit_metadata,
				tx_hash: Default::default(),
			}));
			let starting_epoch = Validator::current_epoch();
			for node in Validator::current_authorities() {
				assert_ok!(Witnesser::witness_at_epoch(
					RuntimeOrigin::signed(node),
					ccm_call.clone(),
					starting_epoch
				));
			}

			// Process the swap -> egress -> threshold sign -> broadcast
			let broadcast_id = 2;
			assert!(EthereumBroadcaster::threshold_signature_data(broadcast_id).is_none());
			testnet.move_forward_blocks(3);

			// Threshold signature is ready and the call is ready to be broadcasted.
			assert!(EthereumBroadcaster::threshold_signature_data(broadcast_id).is_some());

			let validators = Validator::current_authorities();
			let mut broadcast_attempt_id = BroadcastAttemptId { broadcast_id, attempt_count: 0 };

			// Fail the broadcast
			for _ in 0..validators.len() {
				let TransactionSigningAttempt { broadcast_attempt: _attempt, nominee } =
					AwaitingBroadcast::<Runtime, Instance1>::get(broadcast_attempt_id).unwrap();

				assert_ok!(EthereumBroadcaster::transaction_signing_failure(
					RuntimeOrigin::signed(nominee),
					broadcast_attempt_id,
				));
				testnet.move_forward_blocks(1);
				broadcast_attempt_id = broadcast_attempt_id.next_attempt();
			}

			// Upon broadcast failure, the Failure callback is called, and failed CCM is stored.
			assert_eq!(
				EthereumIngressEgress::failed_ccms(broadcast_id),
				vec![FailedCcm { broadcast_id: 2, original_epoch: 2 }]
			);

			// No storage change in the
			testnet.move_forward_blocks(100);
			assert_eq!(
				EthereumIngressEgress::failed_ccms(starting_epoch),
				vec![FailedCcm { broadcast_id: 2, original_epoch: 2 }]
			);

			// On the next epoch, the call is asked to be resigned
			testnet.move_to_the_next_epoch();
			testnet.move_forward_blocks(2);

			assert_eq!(EthereumIngressEgress::failed_ccms(starting_epoch), vec![]);
			assert_eq!(
				EthereumIngressEgress::failed_ccms(starting_epoch + 1),
				vec![FailedCcm { broadcast_id: 2, original_epoch: 2 }]
			);

			// On the next epoch, the failed call is removed from storage.
			testnet.move_to_the_next_epoch();
			testnet.move_forward_blocks(2);
			assert_eq!(EthereumIngressEgress::failed_ccms(starting_epoch), vec![]);
			assert_eq!(EthereumIngressEgress::failed_ccms(starting_epoch + 1), vec![]);
			assert_eq!(EthereumIngressEgress::failed_ccms(starting_epoch + 2), vec![]);

			assert!(ThresholdSignatureData::<Runtime, Instance1>::get(broadcast_id).is_none());
			assert!(RequestFailureCallbacks::<Runtime, Instance1>::get(broadcast_id).is_none());
			assert!(RequestSuccessCallbacks::<Runtime, Instance1>::get(broadcast_id).is_none());
		});
}
