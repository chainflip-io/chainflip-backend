// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{chainflip::generic_elections::ChainlinkOraclePriceSettings, *};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

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
					arb_address_checker: hex_literal::hex!(
						"69c700a0debab9e349dd1f52ed62eb253a3c9892"
					)
					.into(),
					arb_oracle_feeds: vec![
						hex_literal::hex!("6ce185860a4963106506C203335A2910413708e9").into(),
						hex_literal::hex!("639Fe6ab55C921f74e7fac1ee960C0B6293ba612").into(),
						hex_literal::hex!("24ceA4b8ce57cdA5058b924B9B9987992450590c").into(),
						hex_literal::hex!("50834F3163758fcC1Df9973b6e91f0F0F0434aD3").into(),
						hex_literal::hex!("3f3f5dF88dC9F13eac63DF89EC16ef6e7E25DdE7").into(),
					],
					eth_address_checker: hex_literal::hex!(
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
					arb_address_checker: hex_literal::hex!(
						"564e411634189E68ecD570400eBCF783b4aF8688"
					)
					.into(),
					arb_oracle_feeds: vec![
						hex_literal::hex!("56a43EB56Da12C0dc1D972ACb089c06a5dEF8e69").into(),
						hex_literal::hex!("d30e2101a97dcbAeBCBC04F14C3f624E67A35165").into(),
						hex_literal::hex!("32377717BC9F9bA8Db45A244bCE77e7c0Cc5A775").into(),
						hex_literal::hex!("0153002d20B96532C639313c2d54c3dA09109309").into(),
						hex_literal::hex!("80EDee6f667eCc9f63a0a6f55578F870651f06A4").into(),
					],

					eth_address_checker: hex_literal::hex!(
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
					arb_address_checker: hex_literal::hex!(
						"9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"
					)
					.into(),
					arb_oracle_feeds: vec![
						hex_literal::hex!("a85233C63b9Ee964Add6F2cffe00Fd84eb32338f").into(),
						hex_literal::hex!("4A679253410272dd5232B3Ff7cF5dbB88f295319").into(),
						hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F").into(),
						hex_literal::hex!("09635F643e140090A9A8Dcd712eD6285858ceBef").into(),
						hex_literal::hex!("c5a5C42992dECbae36851359345FE25997F5C42d").into(),
					],
					eth_address_checker: hex_literal::hex!(
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

		// Before we initialize the pallet, we kill all `OptionQuery` storage entries. This is
		// because we already released 1.11.x to sisy/persa and it contained a different version
		// of the elections (with sol as extrenal price chain)
		use pallet_cf_elections as elections;
		let _ = elections::SharedDataReferenceCount::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::SharedData::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::BitmapComponents::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::IndividualComponents::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::ElectoralUnsynchronisedStateMap::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::ElectoralSettings::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::ElectionProperties::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::ElectionState::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::ElectionConsensusHistory::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::ElectionConsensusHistoryUpToDate::<Runtime, ()>::clear(u32::MAX, None);
		let _ = elections::ContributingAuthorities::<Runtime, ()>::clear(u32::MAX, None);
		elections::NextElectionIdentifier::<Runtime, ()>::kill();
		elections::ElectoralUnsynchronisedSettings::<Runtime, ()>::kill();
		elections::ElectoralUnsynchronisedState::<Runtime, ()>::kill();
		elections::Status::<Runtime, ()>::kill();

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
