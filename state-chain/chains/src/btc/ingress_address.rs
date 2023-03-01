use bech32::{self, u5, ToBase32, Variant};
use frame_support::sp_io::hashing::sha2_256;
use libsecp256k1::{PublicKey, SecretKey};
use sp_std::vec::Vec;
extern crate alloc;
use alloc::string::String;
use itertools;
use sp_std::iter;

/// For reference see https://developer.bitcoin.org/reference/transactions.html#compactsize-unsigned-integers
fn to_varint(value: u64) -> Vec<u8> {
	let mut result = Vec::default();
	let len = match value {
		0..=0xFC => 1,
		0xFD..=0xFFFF => {
			result.push(0xFD_u8);
			2
		},
		0x010000..=0xFFFFFFFF => {
			result.push(0xFE_u8);
			4
		},
		_ => {
			result.push(0xFF_u8);
			8
		},
	};
	result.extend(value.to_le_bytes().iter().take(len));
	result
}

#[derive(Default)]
struct BitcoinScript(Vec<u8>);

/// For reference see https://en.bitcoin.it/wiki/Script
impl BitcoinScript {
	/// Adds an operation to the script that pushes an unsigned integer onto the stack
	fn push_uint(mut self, value: u32) -> Self {
		match value {
			0 => self.0.push(0),
			1..=16 => self.0.push(0x50 + value as u8),
			_ => {
				let num_bytes =
					sp_std::mem::size_of::<u32>() - (value.leading_zeros() / 8) as usize;
				self = self.push_bytes(&value.to_le_bytes().into_iter().take(num_bytes).collect());
			},
		}
		self
	}
	/// Adds an operation to the script that pushes exactly the provided bytes of data to the stack
	fn push_bytes(mut self, data: &Vec<u8>) -> Self {
		self.0.extend(to_varint(data.len() as u64));
		self.0.extend(data);
		self
	}
	/// Adds an operation to the script that drops the topmost item from the stack
	fn op_drop(mut self) -> Self {
		self.0.push(0x75);
		self
	}
	/// Adds the CHECKSIG operation to the script
	fn op_checksig(mut self) -> Self {
		self.0.push(0xAC);
		self
	}
	/// Serializes the script by returning a single byte for the length
	/// of the script and then the script itself
	fn serialize(self) -> Vec<u8> {
		itertools::chain!(iter::once(self.0.len() as u8), self.0).collect()
	}
}

// Derives a taproot address from a validator public key and a salt
pub fn derive_btc_ingress_address(pubkey_x: [u8; 32], salt: u32) -> String {
	// SHA256("TapLeaf")
	const TAPLEAF_HASH: &[u8] =
		&hex_literal::hex!("aeea8fdc4208983105734b58081d1e2638d35f1cb54008d4d357ca03be78e9ee");
	// SHA256("TapTweak")
	const TAPTWEAK_HASH: &[u8] =
		&hex_literal::hex!("e80fe1639c9ca050e3af1b39c143c63e429cbceb15d940fbb5c5a1f4af57c5e9");
	// A public key that obviously doesn't have a known private key
	const INTERNAL_PUBKEY: &[u8] =
		&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
	let leaf_version = 0xC0_u8;
	let script = BitcoinScript::default()
		.push_uint(salt)
		.op_drop()
		.push_bytes(&pubkey_x.to_vec())
		.op_checksig();
	let leafhash =
		sha2_256(&[TAPLEAF_HASH, TAPLEAF_HASH, &[leaf_version], &script.serialize()].concat());
	let tweakhash =
		sha2_256(&[TAPTWEAK_HASH, TAPTWEAK_HASH, &INTERNAL_PUBKEY[1..33], &leafhash].concat());
	let mut tweaked = PublicKey::parse_compressed(INTERNAL_PUBKEY.try_into().unwrap()).unwrap();
	let _result = tweaked.tweak_add_assign(&SecretKey::parse(&tweakhash).unwrap());
	let segwit_version = u5::try_from_u8(1).unwrap();
	let payload = itertools::chain!(
		iter::once(segwit_version),
		tweaked.serialize_compressed()[1..33].as_ref().to_base32()
	)
	.collect::<Vec<_>>();
	bech32::encode("bc", payload, Variant::Bech32m).unwrap()
}

#[test]
fn test_btc_derive_ingress_address() {
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"),
			0
		),
		"bc1p4syuuy97f96lfah764w33ru9v5u3uk8n8jk9xsq684xfl8sxu82sdcvdcx"
	);
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			15
		),
		"bc1phgs87wzfdqp9amtyc6darrhk3sm38tpf9a39mgjycthcet7vxl3qktqz86"
	);
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			50
		),
		"bc1p2uf6vzdzmv0u7wyfnljnrctr5qr6hy6mmzyjpr6z7x8yt39gppfq3a54c9"
	);
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			123456789
		),
		"bc1p8ea6zrds8q5mke8l6rlrluyle82xdr3sx4dk73r78l859gjfpsrq6gq3ev"
	);
}

#[test]
fn test_build_script() {
	assert_eq!(
		BitcoinScript::default()
			.push_uint(0)
			.op_drop()
			.push_bytes(
				&hex_literal::hex!(
					"2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"
				)
				.to_vec()
			)
			.op_checksig()
			.serialize(),
		hex_literal::hex!(
			"240075202E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105AC"
		)
	);
}

#[test]
fn test_push_uint() {
	let test_data = [
		(0, vec![0]),
		(1, vec![81]),
		(2, vec![82]),
		(16, vec![96]),
		(17, vec![1, 17]),
		(255, vec![1, 255]),
		(256, vec![2, 0, 1]),
		(11394560, vec![3, 0, 0xDE, 0xAD]),
		(u32::MAX, vec![4, 255, 255, 255, 255]),
	];
	for x in test_data {
		assert_eq!(BitcoinScript::default().push_uint(x.0).0, x.1);
	}
}

#[test]
fn test_varint() {
	let test_data = [
		(0_u64, vec![0x00]),
		(1, vec![0x01]),
		(252, vec![0xFC]),
		(253, vec![0xFD, 0xFD, 0x00]),
		(254, vec![0xFD, 0xFE, 0x00]),
		(255, vec![0xFD, 0xFF, 0x00]),
		(65534, vec![0xFD, 0xFE, 0xFF]),
		(65535, vec![0xFD, 0xFF, 0xFF]),
		(65536, vec![0xFE, 0x00, 0x00, 0x01, 0x00]),
		(65537, vec![0xFE, 0x01, 0x00, 0x01, 0x00]),
		(4294967295, vec![0xFE, 0xFF, 0xFF, 0xFF, 0xFF]),
		(4294967296, vec![0xFF, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]),
		(4294967297, vec![0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]),
		(9007199254740991, vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x1F, 0x00]),
	];
	for x in test_data {
		assert_eq!(to_varint(x.0), x.1);
	}
}
