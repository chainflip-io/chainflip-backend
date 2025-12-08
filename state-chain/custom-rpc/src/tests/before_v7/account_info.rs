use super::{account_info_before_api_v7::RpcAccountInfo as RpcAccountInfoBeforeV7, *};
use state_chain_runtime::runtime_apis::validator_info_before_v7::ValidatorInfo as ValidatorInfoBeforeV7;

#[test]
fn test_no_account_serialization() {
	insta::assert_snapshot!(serde_json::to_value(RpcAccountInfoBeforeV7::unregistered(
		0,
		any::AssetMap::default()
	))
	.unwrap());
}

#[test]
fn test_broker_serialization() {
	use cf_chains::btc::BitcoinNetwork;
	let broker = RpcAccountInfoBeforeV7::broker(
		BrokerInfo {
			earned_fees: vec![
				(Asset::Eth, 0),
				(Asset::Btc, 0),
				(Asset::Flip, 1000000000000000000),
				(Asset::Usdc, 0),
				(Asset::Usdt, 0),
				(Asset::Dot, 0),
				(Asset::ArbEth, 0),
				(Asset::ArbUsdc, 0),
				(Asset::Sol, 0),
				(Asset::SolUsdc, 0),
			],
			btc_vault_deposit_address: Some(
				ScriptPubkey::Taproot([1u8; 32]).to_address(&BitcoinNetwork::Testnet),
			),
			bond: 0,
			affiliates: vec![(
				AccountId32::new([1; 32]),
				AffiliateDetails { short_id: 1.into(), withdrawal_address: H160::from([0xcf; 20]) },
			)],
		},
		0,
	);
	insta::assert_json_snapshot!(broker);
}

#[test]
fn test_lp_serialization() {
	let lp = RpcAccountInfoBeforeV7::lp(
		LiquidityProviderInfo {
			refund_addresses: vec![
				(ForeignChain::Ethereum, Some(ForeignChainAddress::Eth(H160::from([1; 20])))),
				(ForeignChain::Polkadot, Some(ForeignChainAddress::Dot(Default::default()))),
				(ForeignChain::Bitcoin, None),
				(ForeignChain::Arbitrum, Some(ForeignChainAddress::Arb(H160::from([2; 20])))),
				(ForeignChain::Solana, None),
			],
			balances: vec![
				(Asset::Eth, u128::MAX),
				(Asset::Btc, 0),
				(Asset::Flip, u128::MAX / 2),
				(Asset::Usdc, 0),
				(Asset::Usdt, 0),
				(Asset::Dot, 0),
				(Asset::ArbEth, 1),
				(Asset::ArbUsdc, 2),
				(Asset::Sol, 3),
				(Asset::SolUsdc, 4),
			],
			earned_fees: any::AssetMap {
				eth: eth::AssetMap {
					eth: 0u32.into(),
					flip: u64::MAX.into(),
					usdc: (u64::MAX / 2 - 1).into(),
					usdt: 0u32.into(),
					wbtc: 0u32.into(),
				},
				btc: btc::AssetMap { btc: 0u32.into() },
				dot: dot::AssetMap { dot: 0u32.into() },
				arb: arb::AssetMap { eth: 1u32.into(), usdc: 2u32.into() },
				sol: sol::AssetMap { sol: 2u32.into(), usdc: 4u32.into() },
				hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
			},
			boost_balances: any::AssetMap {
				btc: btc::AssetMap {
					btc: vec![LiquidityProviderBoostPoolInfo {
						fee_tier: 5,
						total_balance: 100_000_000,
						available_balance: 50_000_000,
						in_use_balance: 50_000_000,
						is_withdrawing: false,
					}],
				},
				..Default::default()
			},
			collateral_balances: vec![(Asset::SolUsdc, 400_000)],
			lending_positions: vec![LendingPosition {
				asset: Asset::Usdt,
				total_amount: 500_000,
				available_amount: 250_000,
			}],
		},
		cf_primitives::NetworkEnvironment::Mainnet,
		0,
	);

	insta::assert_snapshot!(serde_json::to_value(lp).unwrap());
}

#[test]
fn test_validator_serialization() {
	let validator = RpcAccountInfoBeforeV7::validator(ValidatorInfoBeforeV7 {
		balance: FLIPPERINOS_PER_FLIP,
		bond: FLIPPERINOS_PER_FLIP,
		last_heartbeat: 0,
		reputation_points: 0,
		keyholder_epochs: vec![123],
		is_current_authority: true,
		is_bidding: false,
		is_current_backup: false,
		is_online: true,
		is_qualified: true,
		bound_redeem_address: Some(H160::from([1; 20])),
		apy_bp: Some(100u32),
		restricted_balances: BTreeMap::from_iter(vec![(H160::from([1; 20]), FLIPPERINOS_PER_FLIP)]),
		estimated_redeemable_balance: 0,
	});

	insta::assert_snapshot!(serde_json::to_value(validator).unwrap());
}
