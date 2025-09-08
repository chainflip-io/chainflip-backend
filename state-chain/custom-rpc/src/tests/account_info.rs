use super::*;

#[test]
fn test_no_account_serialization() {
	let wrapper = RpcAccountInfoWrapper {
		common_items: RpcAccountInfoCommonItems {
			flip_balance: 1500000000000000000u128.into(), // 1.5 FLIP
			asset_balances: any::AssetMap {
				eth: eth::AssetMap {
					eth: 2500000000000000000u128.into(), // 2.5 ETH
					flip: 0u128.into(),
					usdc: 1000000u128.into(), // 1 USDC
					usdt: 0u128.into(),
				},
				btc: btc::AssetMap { btc: 100000000u128.into() }, // 1 BTC
				dot: dot::AssetMap { dot: 10000000000u128.into() }, // 1 DOT
				arb: arb::AssetMap { eth: 0u128.into(), usdc: 0u128.into() },
				sol: sol::AssetMap { sol: 0u128.into(), usdc: 0u128.into() },
				hub: hub::AssetMap { dot: 0u128.into(), usdc: 0u128.into(), usdt: 0u128.into() },
			},
			bond: 0u32.into(),
			estimated_redeemable_balance: 0u32.into(),
			bound_redeem_address: None,
			restricted_balances: BTreeMap::new(),
			delegating_to: None,
		},
		role_specific: RpcAccountInfo::Unregistered {},
	};
	insta::assert_snapshot!(serde_json::to_string_pretty(&wrapper).unwrap());
}

#[test]
fn test_broker_serialization() {
	use cf_chains::btc::BitcoinNetwork;

	let earned_fees = any::AssetMap {
		eth: eth::AssetMap {
			eth: 250000000000000000u128.into(),   // 0.25 ETH
			flip: 1000000000000000000u128.into(), // 1 FLIP
			usdc: 500000u128.into(),              // 0.5 USDC
			usdt: 300000u128.into(),              // 0.3 USDT
		},
		btc: btc::AssetMap { btc: 5000000u128.into() }, // 0.05 BTC
		dot: dot::AssetMap { dot: 5000000000u128.into() }, // 0.5 DOT
		arb: arb::AssetMap {
			eth: 100000000000000000u128.into(), // 0.1 ETH
			usdc: 200000u128.into(),            // 0.2 USDC
		},
		sol: sol::AssetMap {
			sol: 1000000000u128.into(), // 1 SOL
			usdc: 150000u128.into(),    // 0.15 USDC
		},
		hub: hub::AssetMap {
			dot: 2000000000u128.into(), // 0.2 DOT
			usdc: 100000u128.into(),    // 0.1 USDC
			usdt: 50000u128.into(),     // 0.05 USDT
		},
	};

	let wrapper = RpcAccountInfoWrapper {
		common_items: RpcAccountInfoCommonItems {
			flip_balance: 5000000000000000000u128.into(), // 5 FLIP
			asset_balances: Default::default(),
			bond: 10000000000000000000u128.into(), // 10 FLIP
			estimated_redeemable_balance: 4000000000000000000u128.into(),
			bound_redeem_address: Some(H160::from([0xaa; 20])),
			restricted_balances: BTreeMap::from_iter(vec![
				(H160::from([0xbb; 20]), 2000000000000000000u128.into()), // 2 FLIP restricted
			]),
			delegating_to: None,
		},
		role_specific: RpcAccountInfo::Broker {
			earned_fees,
			affiliates: vec![
				RpcAffiliate {
					account_id: AccountId32::new([1; 32]),
					details: AffiliateDetails {
						short_id: 1u8.into(),
						withdrawal_address: H160::from([0xcf; 20]),
					},
				},
				RpcAffiliate {
					account_id: AccountId32::new([2; 32]),
					details: AffiliateDetails {
						short_id: 42u8.into(),
						withdrawal_address: H160::from([0xde; 20]),
					},
				},
			],
			btc_vault_deposit_address: Some(
				ScriptPubkey::Taproot([1u8; 32]).to_address(&BitcoinNetwork::Testnet),
			),
		},
	};
	insta::assert_snapshot!(serde_json::to_string_pretty(&wrapper).unwrap());
}

#[test]
fn test_lp_serialization() {
	use cf_chains::hub::SubstrateNetworkAddress;
	let refund_addresses = HashMap::from_iter(vec![
		(
			ForeignChain::Ethereum,
			Some(ForeignChainAddressHumanreadable::Eth(H160::from([0x11; 20]))),
		),
		(
			ForeignChain::Polkadot,
			Some(ForeignChainAddressHumanreadable::Dot(SubstrateNetworkAddress::polkadot(
				AccountId32::new([0xcf; 32]),
			))),
		),
		(ForeignChain::Bitcoin, None),
		(
			ForeignChain::Arbitrum,
			Some(ForeignChainAddressHumanreadable::Arb(H160::from([0x22; 20]))),
		),
		(
			ForeignChain::Solana,
			Some(ForeignChainAddressHumanreadable::Sol(
				"11111111111111111111111111111111".to_string(),
			)),
		),
	]);

	let earned_fees = any::AssetMap {
		eth: eth::AssetMap {
			eth: 1000000000000000000u128.into(), // 1 ETH
			flip: u64::MAX.into(),
			usdc: (u64::MAX / 2 - 1).into(),
			usdt: 250000u128.into(), // 0.25 USDT
		},
		btc: btc::AssetMap { btc: 25000000u128.into() }, // 0.25 BTC
		dot: dot::AssetMap { dot: 15000000000u128.into() }, // 1.5 DOT
		arb: arb::AssetMap { eth: 1u128.into(), usdc: 2u128.into() },
		sol: sol::AssetMap { sol: 2u128.into(), usdc: 4u128.into() },
		hub: hub::AssetMap {
			dot: 5000000000u128.into(),
			usdc: 1000000u128.into(),
			usdt: 500000u128.into(),
		},
	};

	let boost_balances = any::AssetMap {
		btc: btc::AssetMap {
			btc: vec![
				RpcLiquidityProviderBoostPoolInfo {
					fee_tier: 5,
					total_balance: 100_000_000u128.into(),
					available_balance: 50_000_000u128.into(),
					in_use_balance: 50_000_000u128.into(),
					is_withdrawing: false,
				},
				RpcLiquidityProviderBoostPoolInfo {
					fee_tier: 10,
					total_balance: 200_000_000u128.into(),
					available_balance: 150_000_000u128.into(),
					in_use_balance: 50_000_000u128.into(),
					is_withdrawing: true,
				},
			],
		},
		..Default::default()
	};

	let asset_balances = any::AssetMap {
		eth: eth::AssetMap {
			eth: u128::MAX.into(),
			flip: (u128::MAX / 2).into(),
			usdc: 10000000u128.into(), // 10 USDC
			usdt: 5000000u128.into(),  // 5 USDT
		},
		btc: btc::AssetMap { btc: 500000000u128.into() }, // 5 BTC
		dot: dot::AssetMap { dot: 100000000000u128.into() }, // 10 DOT
		arb: arb::AssetMap { eth: 1u128.into(), usdc: 2u128.into() },
		sol: sol::AssetMap { sol: 3u128.into(), usdc: 4u128.into() },
		hub: hub::AssetMap {
			dot: 50000000000u128.into(),
			usdc: 3000000u128.into(),
			usdt: 2000000u128.into(),
		},
	};

	let wrapper = RpcAccountInfoWrapper {
		common_items: RpcAccountInfoCommonItems {
			flip_balance: 25000000000000000000u128.into(), // 25 FLIP
			asset_balances,
			bond: 0u32.into(),
			estimated_redeemable_balance: 1000000000000000000u128.into(), // 1 FLIP
			bound_redeem_address: None,
			restricted_balances: BTreeMap::new(),
			delegating_to: Some(AccountId32::new([0x33; 32])),
		},
		role_specific: RpcAccountInfo::LiquidityProvider {
			refund_addresses,
			earned_fees,
			boost_balances,
		},
	};

	insta::assert_snapshot!(serde_json::to_string_pretty(&wrapper).unwrap());
}

#[test]
fn test_validator_serialization() {
	let wrapper = RpcAccountInfoWrapper {
		common_items: RpcAccountInfoCommonItems {
			flip_balance: (FLIPPERINOS_PER_FLIP * 50).into(), // 50 FLIP
			asset_balances: any::AssetMap {
				eth: eth::AssetMap {
					eth: 3000000000000000000u128.into(), // 3 ETH
					flip: 0u128.into(),
					usdc: 2500000u128.into(), // 2.5 USDC
					usdt: 1500000u128.into(), // 1.5 USDT
				},
				btc: btc::AssetMap { btc: 75000000u128.into() }, // 0.75 BTC
				dot: dot::AssetMap { dot: 25000000000u128.into() }, // 2.5 DOT
				arb: arb::AssetMap { eth: 500000000000000000u128.into(), usdc: 1000000u128.into() },
				sol: sol::AssetMap { sol: 5000000000u128.into(), usdc: 750000u128.into() },
				hub: hub::AssetMap {
					dot: 10000000000u128.into(),
					usdc: 500000u128.into(),
					usdt: 250000u128.into(),
				},
			},
			bond: (FLIPPERINOS_PER_FLIP * 100).into(), // 100 FLIP
			estimated_redeemable_balance: (FLIPPERINOS_PER_FLIP * 25).into(), // 25 FLIP
			bound_redeem_address: Some(H160::from([0x44; 20])),
			restricted_balances: BTreeMap::from_iter(vec![
				(H160::from([0x55; 20]), (FLIPPERINOS_PER_FLIP * 10).into()), // 10 FLIP
				(H160::from([0x66; 20]), (FLIPPERINOS_PER_FLIP * 5).into()),  // 5 FLIP
			]),
			delegating_to: None,
		},
		role_specific: RpcAccountInfo::Validator {
			last_heartbeat: 150000,
			reputation_points: 850,
			keyholder_epochs: vec![100, 123, 124, 125],
			is_current_authority: true,
			is_bidding: false,
			is_current_backup: false,
			is_online: true,
			is_qualified: true,
			apy_bp: Some(375u32), // 3.75% APY
			operator: Some(AccountId32::new([0x77; 32])),
		},
	};

	insta::assert_snapshot!(serde_json::to_string_pretty(&wrapper).unwrap());
}
