#![cfg(test)]

use crate::runtime_apis::{
	custom_api::test_all_historical_runtime_calls,
	historical_compatibility::{
		offline_metadata_tester::OfflineMetadataTester, online_node_tester::OnlineNodeTester,
	},
};

#[test]
pub fn offline_test_historical_compatibility_of_runtime_api() {
	let mut tester = OfflineMetadataTester::new();
	let incompatibilities = test_all_historical_runtime_calls(&mut tester, file!());

	if incompatibilities.len() > 0 {
		for incompatibility in incompatibilities {
			println!("{incompatibility}");
			println!("");
		}

		panic!("type schema incompatibilities found!")
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
