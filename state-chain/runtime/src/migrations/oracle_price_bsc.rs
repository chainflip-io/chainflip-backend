// Copyright 2026 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! Resets the generic oracle elections with the BSC price source enabled.

use crate::{
	chainflip::witnessing::generic_elections::{initial_state, ChainlinkOraclePriceSettings},
	Runtime,
};
use cf_primitives::ChainflipNetwork;
#[cfg(feature = "try-runtime")]
use frame_support::traits::GetStorageVersion;
use frame_support::{
	migrations::VersionedMigration as FrameVersionedMigration, traits::UncheckedOnRuntimeUpgrade,
	weights::Weight,
};
use pallet_cf_elections::InitialStateOf;
#[cfg(feature = "try-runtime")]
use pallet_cf_elections::UniqueMonotonicIdentifier;
use sp_core::H160;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub type VersionedMigration = FrameVersionedMigration<
	9,
	10,
	Migration,
	pallet_cf_elections::Pallet<Runtime>,
	<Runtime as frame_system::Config>::DbWeight,
>;

pub struct Migration;

impl UncheckedOnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		let initial =
			new_initial_state(pallet_cf_environment::ChainflipNetworkName::<Runtime>::get());

		pallet_cf_elections::Pallet::<Runtime>::reset();
		if let Err(error) = pallet_cf_elections::Pallet::<Runtime>::internally_initialize(initial) {
			cf_runtime_utilities::log_or_panic!(
				"Failed to initialize generic oracle elections after reset: {error:?}"
			);
		}

		log::info!("Reset generic oracle elections with the BSC price source enabled");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let expected =
			new_initial_state(pallet_cf_environment::ChainflipNetworkName::<Runtime>::get());
		frame_support::ensure!(
			pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime>::get() ==
				Some(expected.unsynchronised_state),
			"Generic oracle state was not reset to its initial value"
		);
		frame_support::ensure!(
			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime>::get() ==
				Some(expected.unsynchronised_settings),
			"Generic oracle settings were not reset to their initial value"
		);

		let (expected_electoral_settings,) = expected.settings;
		let (electoral_settings,) = pallet_cf_elections::ElectoralSettings::<Runtime>::get(
			UniqueMonotonicIdentifier::default(),
		)
		.ok_or("Reset oracle feed settings are missing")?;
		frame_support::ensure!(
			electoral_settings == expected_electoral_settings,
			"Oracle feed settings differ from the chain-spec values"
		);
		frame_support::ensure!(
			pallet_cf_elections::ElectoralSettings::<Runtime>::iter().count() == 1,
			"Old oracle settings boundaries were not cleared"
		);
		frame_support::ensure!(
			election_storage_is_empty(),
			"Old oracle elections were not cleared"
		);
		frame_support::ensure!(
			pallet_cf_elections::NextElectionIdentifier::<Runtime>::get() ==
				UniqueMonotonicIdentifier::default(),
			"Oracle election identifiers were not reset"
		);
		frame_support::ensure!(
			pallet_cf_elections::ContributingAuthorities::<Runtime>::iter_keys()
				.next()
				.is_none(),
			"Contributing oracle authorities were not cleared"
		);
		frame_support::ensure!(
			pallet_cf_elections::Status::<Runtime>::get() ==
				Some(pallet_cf_elections::ElectionPalletStatus::Running),
			"Generic oracle elections were not initialized"
		);
		frame_support::ensure!(
			pallet_cf_elections::SharedDataReferenceLifetime::<Runtime>::get() ==
				expected.shared_data_reference_lifetime,
			"Oracle shared data reference lifetime was not initialized"
		);
		frame_support::ensure!(
			pallet_cf_elections::Pallet::<Runtime>::on_chain_storage_version() ==
				pallet_cf_elections::STORAGE_VERSION,
			"Generic Elections storage version was not updated"
		);
		pallet_cf_elections::Pallet::<Runtime>::do_try_state()?;

		Ok(())
	}
}

fn new_initial_state(network: ChainflipNetwork) -> InitialStateOf<Runtime, ()> {
	let OracleFeeds { arbitrum, ethereum, bsc } = oracle_feeds(network);
	initial_state(ChainlinkOraclePriceSettings {
		arb_address_checker: pallet_cf_environment::ArbitrumAddressCheckerAddress::<Runtime>::get(),
		arb_oracle_feeds: arbitrum.into(),
		eth_address_checker: pallet_cf_environment::EthereumAddressCheckerAddress::<Runtime>::get(),
		eth_oracle_feeds: ethereum.into(),
		bsc_address_checker: pallet_cf_environment::BscAddressCheckerAddress::<Runtime>::get(),
		bsc_oracle_feeds: bsc.into(),
	})
}

struct OracleFeeds {
	arbitrum: [H160; 8],
	ethereum: [H160; 7],
	bsc: [H160; 8],
}

// These are the complete feed lists from the corresponding chain specs.
fn oracle_feeds(network: ChainflipNetwork) -> OracleFeeds {
	match network {
		// Berghain
		ChainflipNetwork::Mainnet => OracleFeeds {
			arbitrum: [
				// Btc
				H160(hex_literal::hex!("6ce185860a4963106506C203335A2910413708e9")),
				// Eth
				H160(hex_literal::hex!("639Fe6ab55C921f74e7fac1ee960C0B6293ba612")),
				// Sol
				H160(hex_literal::hex!("24ceA4b8ce57cdA5058b924B9B9987992450590c")),
				// Usdc
				H160(hex_literal::hex!("50834F3163758fcC1Df9973b6e91f0F0F0434aD3")),
				// Usdt
				H160(hex_literal::hex!("3f3f5dF88dC9F13eac63DF89EC16ef6e7E25DdE7")),
				// Wbtc
				H160(hex_literal::hex!("d0C7101eACbB49F3deCcCc166d238410D6D46d57")),
				// Bnb
				H160(hex_literal::hex!("6970460aabF80C5BE983C6b74e5D06dEDCA95D4A")),
				// Dot
				H160(hex_literal::hex!("a6bC5bAF2000424e90434bA7104ee399dEe80DEc")),
			],
			ethereum: [
				// Btc
				H160(hex_literal::hex!("F4030086522a5bEEa4988F8cA5B36dbC97BeE88c")),
				// Eth
				H160(hex_literal::hex!("5f4eC3Df9cbd43714FE2740f5E3616155c5b8419")),
				// Sol
				H160(hex_literal::hex!("4ffC43a60e009B551865A93d232E33Fce9f01507")),
				// Usdc
				H160(hex_literal::hex!("8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6")),
				// Usdt
				H160(hex_literal::hex!("3E7d1eAB13ad0104d2750B8863b489D65364e32D")),
				// Cbbtc
				H160(hex_literal::hex!("2665701293fCbEB223D11A08D826563EDcCE423A")),
				// Bnb
				H160(hex_literal::hex!("14e613AC84a31f709eadbdF89C6CC390fDc9540A")),
			],
			bsc: [
				// Btc
				H160(hex_literal::hex!("264990fbd0A4796A3E3d8E37C4d5F87a3aCa5Ebf")),
				// Eth
				H160(hex_literal::hex!("9ef1B8c0E4F7dc8bF5719Ea496883DC6401d5b2e")),
				// Sol
				H160(hex_literal::hex!("0E8a53DD9c13589df6382F13dA6B3Ec8F919B323")),
				// Usdc
				H160(hex_literal::hex!("51597f405303C4377E36123cBc172b13269EA163")),
				// Usdt
				H160(hex_literal::hex!("B97Ad0E74fa7d920791E90258A6E2085088b4320")),
				// Trx
				H160(hex_literal::hex!("F4C5e535756D11994fCBB12Ba8adD0192D9b88be")),
				// Bnb
				H160(hex_literal::hex!("0567F2323251f0Aab15c8dFb1967E4e8A7D42aeE")),
				// Dot
				H160(hex_literal::hex!("C333eb0086309a16aa7c8308DfD32c8BBA0a2592")),
			],
		},
		// Perseverance
		ChainflipNetwork::Testnet => perseverance_and_sisyphos_oracle_feeds(),
		// Sisyphos
		ChainflipNetwork::TestnetDev => perseverance_and_sisyphos_oracle_feeds(),
		// Local development
		ChainflipNetwork::Development => OracleFeeds {
			arbitrum: [
				// Btc
				H160(hex_literal::hex!("a85233C63b9Ee964Add6F2cffe00Fd84eb32338f")),
				// Eth
				H160(hex_literal::hex!("4A679253410272dd5232B3Ff7cF5dbB88f295319")),
				// Sol
				H160(hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F")),
				// Usdc
				H160(hex_literal::hex!("09635F643e140090A9A8Dcd712eD6285858ceBef")),
				// Usdt
				H160(hex_literal::hex!("c5a5C42992dECbae36851359345FE25997F5C42d")),
				// Wbtc
				H160(hex_literal::hex!("c3e53F4d16Ae77Db1c982e75a937B9f60FE63690")),
				// Bnb
				H160(hex_literal::hex!("84eA74d481Ee0A5332c457a4d796187F6Ba67fEB")),
				// Dot
				H160(hex_literal::hex!("9E545E3C0baAB3E08CdfD552C960A1050f373042")),
			],
			ethereum: [
				// Btc
				H160(hex_literal::hex!("322813Fd9A801c5507c9de605d63CEA4f2CE6c44")),
				// Eth
				H160(hex_literal::hex!("a85233C63b9Ee964Add6F2cffe00Fd84eb32338f")),
				// Sol
				H160(hex_literal::hex!("4A679253410272dd5232B3Ff7cF5dbB88f295319")),
				// Usdc
				H160(hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F")),
				// Usdt
				H160(hex_literal::hex!("09635F643e140090A9A8Dcd712eD6285858ceBef")),
				// Cbbtc
				H160(hex_literal::hex!("c3e53F4d16Ae77Db1c982e75a937B9f60FE63690")),
				// Bnb
				H160(hex_literal::hex!("84eA74d481Ee0A5332c457a4d796187F6Ba67fEB")),
			],
			bsc: [
				// Btc
				H160(hex_literal::hex!("5FC8d32690cc91D4c39d9d3abcBD16989F875707")),
				// Eth
				H160(hex_literal::hex!("0165878A594ca255338adfa4d48449f69242Eb8F")),
				// Sol
				H160(hex_literal::hex!("a513E6E4b8f2a923D98304ec87F64353C4D5C853")),
				// Usdc
				H160(hex_literal::hex!("2279B7A0a67DB372996a5FaB50D91eAA73d2eBe6")),
				// Usdt
				H160(hex_literal::hex!("8A791620dd6260079BF849Dc5567aDC3F2FdC318")),
				// Trx
				H160(hex_literal::hex!("610178dA211FEF7D417bC0e6FeD39F05609AD788")),
				// Bnb
				H160(hex_literal::hex!("B7f8BC63BbcaD18155201308C8f3540b07f84F5e")),
				// Dot
				H160(hex_literal::hex!("A51c1fc2f0D1a1b8494Ed1FE312d7C3a78Ed91C0")),
			],
		},
	}
}

// Perseverance and Sisyphos currently use the same feed addresses.
fn perseverance_and_sisyphos_oracle_feeds() -> OracleFeeds {
	OracleFeeds {
		arbitrum: [
			// Btc
			H160(hex_literal::hex!("56a43EB56Da12C0dc1D972ACb089c06a5dEF8e69")),
			// Eth
			H160(hex_literal::hex!("d30e2101a97dcbAeBCBC04F14C3f624E67A35165")),
			// Sol
			H160(hex_literal::hex!("32377717BC9F9bA8Db45A244bCE77e7c0Cc5A775")),
			// Usdc
			H160(hex_literal::hex!("0153002d20B96532C639313c2d54c3dA09109309")),
			// Usdt
			H160(hex_literal::hex!("80EDee6f667eCc9f63a0a6f55578F870651f06A4")),
			// Wbtc (BTC feed)
			H160(hex_literal::hex!("56a43EB56Da12C0dc1D972ACb089c06a5dEF8e69")),
			// Bnb (ETH feed)
			H160(hex_literal::hex!("d30e2101a97dcbAeBCBC04F14C3f624E67A35165")),
			// Dot (SOL feed)
			H160(hex_literal::hex!("32377717BC9F9bA8Db45A244bCE77e7c0Cc5A775")),
		],
		ethereum: [
			// Btc
			H160(hex_literal::hex!("1b44F3514812d835EB1BDB0acB33d3fA3351Ee43")),
			// Eth
			H160(hex_literal::hex!("694AA1769357215DE4FAC081bf1f309aDC325306")),
			// Sol (ETH feed)
			H160(hex_literal::hex!("694AA1769357215DE4FAC081bf1f309aDC325306")),
			// Usdc
			H160(hex_literal::hex!("A2F78ab2355fe2f984D808B5CeE7FD0A93D5270E")),
			// Usdt (USDC feed)
			H160(hex_literal::hex!("A2F78ab2355fe2f984D808B5CeE7FD0A93D5270E")),
			// Cbbtc (BTC feed)
			H160(hex_literal::hex!("1b44F3514812d835EB1BDB0acB33d3fA3351Ee43")),
			// Bnb (ETH feed)
			H160(hex_literal::hex!("694AA1769357215DE4FAC081bf1f309aDC325306")),
		],
		bsc: [
			// Btc (BNB feed)
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
			// Eth (BNB feed)
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
			// Sol (BNB feed)
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
			// Usdc (BNB feed)
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
			// Usdt (BNB feed)
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
			// Trx (BNB feed)
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
			// Bnb
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
			// Dot (BNB feed)
			H160(hex_literal::hex!("2514895c72f50D8bd4B4F9b1110F0D6bD2c97526")),
		],
	}
}

#[cfg(feature = "try-runtime")]
fn election_storage_is_empty() -> bool {
	pallet_cf_elections::SharedDataReferenceCount::<Runtime>::iter()
		.next()
		.is_none() &&
		pallet_cf_elections::SharedData::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::BitmapComponents::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::IndividualComponents::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::ElectoralUnsynchronisedStateMap::<Runtime>::iter()
			.next()
			.is_none() &&
		pallet_cf_elections::ElectionProperties::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::ElectionState::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::ElectionConsensusHistory::<Runtime>::iter()
			.next()
			.is_none() &&
		pallet_cf_elections::ElectionConsensusHistoryUpToDate::<Runtime>::iter()
			.next()
			.is_none()
}
