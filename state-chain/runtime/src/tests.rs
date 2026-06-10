#![cfg(test)]

use std::collections::HashMap;

use crate::runtime_apis::{
	custom_api::test_all_historical_runtime_calls,
	historical_compatibility::{
		offline_metadata_tester::OfflineMetadataTester,
		online_node_tester::OnlineNodeTester,
		tester_trait::{FullTypeLocation, TypeDiff, TypeIncompatibilityInfo, TypeName},
	},
};

#[test]
pub fn offline_test_historical_compatibility_of_runtime_api() -> Result<(), String> {
	check_incompatibilities(test_all_historical_runtime_calls(&mut OfflineMetadataTester::new()))
}

#[ignore = "requires access to archive node"]
#[test]
pub fn online_test_historical_compatibility_of_runtime_api() -> Result<(), String> {
	check_incompatibilities(test_all_historical_runtime_calls(&mut OnlineNodeTester {
		get_blockhash_from_spec_version: Box::new(|spec_version| match spec_version {
			20012 => Some("0xc2068ad859fc5c3b3c7c5ecb3bd84033f1b5a0ce60e8c3b52cab4d22840eec37"),
			20119 => Some("0x2ad1dd83839b13039d1a4ee85932b439e041068bd3bb91acf43455db97d71bd0"),
			_ => None,
		}),
		node_url: "https://mainnet-archive.chainflip.io",
	}))
}

fn check_incompatibilities(incompatibilities: Vec<TypeIncompatibilityInfo>) -> Result<(), String> {
	if incompatibilities.is_empty() {
		return Ok(());
	}

	let mut types_and_locations: HashMap<(TypeName, TypeDiff), Vec<FullTypeLocation>> =
		Default::default();

	for incompatibility in &incompatibilities {
		types_and_locations
			.entry((
				incompatibility.sub_type_incompat.sub_type_details.type_name.clone(),
				incompatibility.type_diff.clone(),
			))
			.or_default()
			.push(FullTypeLocation {
				reference: incompatibility.type_ref,
				sub_location: incompatibility.sub_type_incompat.sub_type_details.location,
			});

		println!("Full diff:");
		println!("```");
		print!("{}", &incompatibility.type_diff);
		println!("```");
		println!("original error: {}", incompatibility.sub_type_incompat.error);
		println!("");
	}

	println!("# Summary");
	for ((ty, diff), locs) in &types_and_locations {
		println!("{ty}");
		let summary = diff.get_summary();
		print!("{summary}");
		println!("  in: [");
		for l in locs {
			println!("    {l},");
		}
		println!("  ]");
		println!("");
	}

	Err(format!("{} type schema incompatibilities found!", incompatibilities.len()))
}
