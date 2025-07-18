use crate::{chainflip::generic_elections::ChainlinkOraclePriceSettings, *};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};
use sol_prim::consts::const_address;

use crate::chainflip::generic_elections;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let chainlink_oracle_price_settings =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<Runtime>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => ChainlinkOraclePriceSettings {
					sol_oracle_program_id: const_address(
						"HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny",
					),
					sol_oracle_feeds: vec![
						const_address("Cv4T27XbjVoKUYwP72NQQanvZeA7W4YF9L4EnYT9kx5o"),
						const_address("716hFAECqotxcXcj8Hs8nr7AG6q9dBw2oX3k3M8V7uGq"),
						const_address("CH31Xns5z3M1cTAbKW34jcxPPciazARpijcHj9rxtemt"),
						const_address("GzGuoKXE8Unn7Vcg1DtomwD27tL4bVUpSK2M1yk6Xfz5"),
						const_address("8vAuuqC5wVZ9Z9oQUGGDSjYgudTfjmyqGU5VucQxTk5U"),
					],
					sol_oracle_query_helper: const_address(
						"5Vg6D87L4LMDoyze9gU56NhvcRKWrwbJMquF2tj4vnuX",
					),
					eth_contract_address: hex_literal::hex!(
						"1562Ad6bb0e68980A3111F24531c964c7e155611"
					)
					.into(),
					eth_oracle_feeds: vec![
						hex_literal::hex!("F4030086522a5bEEa4988F8cA5B36dbC97BeE88c").into(),
						hex_literal::hex!("5f4eC3Df9cbd43714FE2740f5E3616155c5b8419").into(),
						hex_literal::hex!("4ffC43a60e009B551865A93d232E33Fce9f01507").into(),
						hex_literal::hex!("8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6").into(),
						hex_literal::hex!("3E7d1eAB13ad0104d2750B8863b489D65364e32D").into(),
					],
				},
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE |
				cf_runtime_utilities::genesis_hashes::SISYPHOS => ChainlinkOraclePriceSettings {
					sol_oracle_program_id: const_address(
						"HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny",
					),
					sol_oracle_feeds: vec![
						const_address("6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe"),
						const_address("669U43LNHx7LsVj95uYksnhXUfWKDsdzVqev3V4Jpw3P"),
						const_address("99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR"),
						const_address("2EmfL3MqL3YHABudGNmajjCpR13NNEn9Y4LWxbDm6SwR"),
						const_address("8QQSUPtdRTboa4bKyMftVNRfGFsB4Vp9d7r39hGKi53e"),
					],
					sol_oracle_query_helper: const_address(
						"5Vg6D87L4LMDoyze9gU56NhvcRKWrwbJMquF2tj4vnuX",
					),
					eth_contract_address: hex_literal::hex!(
						"26061f315570bddf11d9055411a3d811c5ff0148"
					)
					.into(),
					eth_oracle_feeds: vec![
						hex_literal::hex!("1b44F3514812d835EB1BDB0acB33d3fA3351Ee43").into(),
						hex_literal::hex!("694AA1769357215DE4FAC081bf1f309aDC325306").into(),
						// There is no SOL price feed in testnet - using ETH instead
						hex_literal::hex!("694AA1769357215DE4FAC081bf1f309aDC325306").into(),
						hex_literal::hex!("A2F78ab2355fe2f984D808B5CeE7FD0A93D5270E").into(),
						// There is no USDT price feed in testnet - using USDC instead
						hex_literal::hex!("A2F78ab2355fe2f984D808B5CeE7FD0A93D5270E").into(),
					],
				},
				// localnet:
				_ => ChainlinkOraclePriceSettings {
					sol_oracle_program_id: const_address(
						"DfYdrym1zoNgc6aANieNqj9GotPj2Br88rPRLUmpre7X",
					),
					sol_oracle_feeds: vec![
						const_address("HDSV2wFxmsrmCwwY34QzaVkvmJpG7VF8S9fX2iThynjG"),
						const_address("8U3c4SqXaXKPQiarNH3xHXiVoBLYbkqkzusthyJJjGrE"),
						const_address("CrjmdLxTkmd5bxTQjE82FNgiuxeoY3G4EzzhDJ4RH9Wx"),
						const_address("7BH1paBwjVDrHTb8YkHcyt7ZfxsCbnBMeByGBH6L8PFk"),
						const_address("7qdy4DhvG5GDkiGNrsmrMcCyiVNPtmrUmGo3UntcrLwk"),
					],
					sol_oracle_query_helper: const_address(
						"GXn7uzbdNgozXuS8fEbqHER1eGpD9yho7FHTeuthWU8z",
					),
					eth_contract_address: hex_literal::hex!(
						"e7f1725E7734CE288F8367e1Bb143E90bb3F0512"
					)
					.into(),
					eth_oracle_feeds: vec![
						hex_literal::hex!("322813Fd9A801c5507c9de605d63CEA4f2CE6c44").into(),
						hex_literal::hex!("a85233C63b9Ee964Add6F2cffe00Fd84eb32338f").into(),
						hex_literal::hex!("4A679253410272dd5232B3Ff7cF5dbB88f295319").into(),
						hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F").into(),
						hex_literal::hex!("09635F643e140090A9A8Dcd712eD6285858ceBef").into(),
					],
				},
			};

		let _result = pallet_cf_elections::Pallet::<Runtime, ()>::internally_initialize(
			generic_elections::initial_state(chainlink_oracle_price_settings),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use pallet_cf_elections::SharedDataReferenceLifetime;

		let lifetime = SharedDataReferenceLifetime::<Runtime, ()>::get();
		assert_eq!(lifetime, 8);
		Ok(())
	}
}
