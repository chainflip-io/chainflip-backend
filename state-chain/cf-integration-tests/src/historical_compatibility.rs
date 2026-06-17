pub mod offline_metadata_tester;
pub mod online_node_tester;
pub mod tester_trait;
pub mod type_describer;

use std::collections::HashMap;

use cf_primitives::FlipBalance;
use cf_utilities::{
	for_each_released_runtime_version,
	migrations::{basics::Version, v20000, v20100, v20200},
};
use frame_support::sp_runtime::AccountId32;
use state_chain_runtime::runtime_apis::custom_api::types::{
	NetworkFees, RpcAccountInfoCommonItems, ShouldSweep,
};

use crate::historical_compatibility::{
	offline_metadata_tester::OfflineMetadataTester,
	online_node_tester::OnlineNodeTester,
	tester_trait::{
		FullTypeLocation, HistoricalCompatibilityTester, TypeDiff, TypeIncompatibilityInfo,
		TypeName,
	},
};

#[test]
pub fn offline_test_historical_compatibility_of_runtime_api() -> Result<(), String> {
	check_incompatibilities(test_all_historical_runtime_calls(
		&mut OfflineMetadataTester::default(),
		|version| version >= 20100, /* runtimes older than that don't support the required V15
		                             * metadata */
	))
}

#[ignore = "requires access to archive node"]
#[test]
pub fn online_test_historical_compatibility_of_runtime_api() -> Result<(), String> {
	check_incompatibilities(test_all_historical_runtime_calls(
		&mut OnlineNodeTester {
			get_blockhash_from_spec_version: Box::new(|spec_version| match spec_version {
				20012 => Some("0xc2068ad859fc5c3b3c7c5ecb3bd84033f1b5a0ce60e8c3b52cab4d22840eec37"),
				20119 => Some("0x2ad1dd83839b13039d1a4ee85932b439e041068bd3bb91acf43455db97d71bd0"),
				_ => None,
			}),
			node_url: "https://mainnet-archive.chainflip.io",
		},
		|_version| true,
	))
}

fn check_incompatibilities(incompatibilities: Vec<TypeIncompatibilityInfo>) -> Result<(), String> {
	if incompatibilities.is_empty() {
		return Ok(());
	}

	const BLUE: &str = "\x1b[94m";
	const RESET: &str = "\x1b[0m";

	let mut types_and_locations: HashMap<(TypeName, TypeDiff), Vec<(usize, FullTypeLocation)>> =
		Default::default();

	for (diff_number, incompatibility) in incompatibilities.iter().enumerate() {
		types_and_locations
			.entry((
				incompatibility.sub_type_incompat.sub_type_details.type_name.clone(),
				incompatibility.type_diff.clone(),
			))
			.or_default()
			.push((
				diff_number,
				FullTypeLocation {
					reference: incompatibility.type_ref,
					sub_location: incompatibility.sub_type_incompat.sub_type_details.location,
				},
			));

		println!("{BLUE}# Diff {diff_number}:{RESET}");
		println!("```");
		print!("{}", &incompatibility.type_diff);
		println!("```");
		println!("original error: {}", incompatibility.sub_type_incompat.error);
		println!("");
	}

	println!("{BLUE}# Summary{RESET}");
	for ((ty, diff), locs) in &types_and_locations {
		println!("{ty}");
		let summary = diff.get_summary();
		print!("{summary}");
		println!("  in: [");
		for (diff_number, location) in locs {
			println!("    {location} (see {BLUE}Diff {diff_number}{RESET}),");
		}
		println!("  ]");
		println!("");
	}

	println!("{BLUE}# Explanation{RESET}");
	println!("The listed types were changed but their changelog wasn't updated.");
	println!("");
	println!("The implementation of the trait `HasChangelog` for a type allows auto-generation of migrations, and also fuzzy testing these \"historical\" types against actual historical metadata.");
	println!("The summary above lists all incompatibilities that were found and provides details: the type, the runtime API call it is referenced in, the historical runtime version that was tested");
	println!("and links to the full diff between what was expected and what was encountered.");
	println!("");
	println!("Please update the `HasChangelog` implementation for the types that are listed. See the documentation of `HasChangelog` for more information.");
	println!("");

	Err(format!("{} type schema incompatibilities", incompatibilities.len()))
}

fn test_all_historical_runtime_calls(
	tester: &mut impl HistoricalCompatibilityTester,
	should_check_version: impl Fn(u32) -> bool,
) -> Vec<TypeIncompatibilityInfo> {
	let mut all_incompatibilities = Vec::new();

	macro_rules! try_test_all_runtime_calls_at_version {
		($version:ident) => {
            if should_check_version(
				$version::CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST
					.expect("Encountered `CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST = None` when trying to run compatibility tests for historical runtime.")
			) {
                let mut incompatibilities = [
                    tester.test_call::<$version, (), NetworkFees>($version, "CustomRuntimeApi", "cf_network_fees"),
                    tester
                        .test_call::<$version, (AccountId32, ShouldSweep), RpcAccountInfoCommonItems<FlipBalance>>(
                            $version,
                            "CustomRuntimeApi",
                            "cf_common_account_info",
                        ),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
                all_incompatibilities.append(&mut incompatibilities);
            }
		};
	}

	for_each_released_runtime_version!(try_test_all_runtime_calls_at_version);

	all_incompatibilities
}
