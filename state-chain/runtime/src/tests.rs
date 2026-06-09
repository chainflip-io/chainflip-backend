#![cfg(test)]

use std::collections::HashMap;

use crate::runtime_apis::{
	custom_api::test_all_historical_runtime_calls,
	historical_compatibility::{
		offline_metadata_tester::OfflineMetadataTester,
		online_node_tester::OnlineNodeTester,
		tester_trait::{SubTypeLocation, TypeName, TypeRef},
	},
};

#[test]
pub fn offline_test_historical_compatibility_of_runtime_api() -> Result<(), String> {
	let mut tester = OfflineMetadataTester::new();
	let incompatibilities = test_all_historical_runtime_calls(&mut tester, file!());

	if incompatibilities.len() > 0 {
		let mut types_and_locations: HashMap<TypeName, Vec<(TypeRef, SubTypeLocation)>> =
			Default::default();

		for incompatibility in &incompatibilities {
			println!("{incompatibility}");
			println!("");

			types_and_locations
				.entry(incompatibility.sub_type_incompat.sub_type_details.type_name.clone())
				.or_default()
				.push((
					incompatibility.type_ref,
					incompatibility.sub_type_incompat.sub_type_details.location,
				));
		}

		println!("The following type mismatches were found:");
		for (ty, locs) in &types_and_locations {
			println!(" - {ty}");
			for (r, l) in locs {
				println!("    - in {r} {l}");
			}
		}

		Err("type schema incompatibilities found!".into())
	} else {
		Ok(())
	}
}

#[ignore = "requires access to archive node"]
#[test]
pub fn online_test_historical_compatibility_of_runtime_api() {
	let mut tester = OnlineNodeTester {
		get_blockhash_from_spec_version: Box::new(|spec_version| match spec_version {
			20012 => Some("0xc2068ad859fc5c3b3c7c5ecb3bd84033f1b5a0ce60e8c3b52cab4d22840eec37"),
			20119 => Some("0x2ad1dd83839b13039d1a4ee85932b439e041068bd3bb91acf43455db97d71bd0"),
			_ => None,
		}),
		node_url: "https://mainnet-archive.chainflip.io",
	};
	test_all_historical_runtime_calls(&mut tester, file!());
}
