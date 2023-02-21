use bech32::{self, u5, ToBase32, Variant};
use frame_support::sp_io::hashing::sha2_256;
use libsecp256k1::{PublicKey, SecretKey};
use sp_std::{vec, vec::Vec};
extern crate alloc;
use alloc::string::String;

const TAPLEAF_HASH: &[u8] =
	&hex_literal::hex!("aeea8fdc4208983105734b58081d1e2638d35f1cb54008d4d357ca03be78e9ee");
const TAPTWEAK_HASH: &[u8] =
	&hex_literal::hex!("e80fe1639c9ca050e3af1b39c143c63e429cbceb15d940fbb5c5a1f4af57c5e9");
const INTERNAL_PUBKEY: &[u8] =
	&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

#[derive(Default)]
struct BitcoinScript(Vec<u8>);

impl BitcoinScript {
	/// Adds an operation to the script that pushes an unsigned integer onto the stack
	fn push_uint(&mut self, value: u32) -> &mut Self {
		match value {
			0 => self.0.push(0),
			1..=16 => self.0.push(0x50 + value as u8),
			_ => {
				let num_bytes = (4 - value.leading_zeros() / 8) as usize;
				self.0.push(num_bytes as u8);
				self.0.append(&mut value.to_le_bytes()[..num_bytes].into());
			},
		}
		self
	}
	/// Adds an operation to the script that pushes exactly 32 bytes of data to the stack
	fn push_32bytes(&mut self, hash: [u8; 32]) -> &mut Self {
		self.0.push(0x20);
		self.0.append(&mut hash.into());
		self
	}
	/// Adds an operation to the script that drops the topmost item from the stack
	fn op_drop(&mut self) -> &mut Self {
		self.0.push(0x75);
		self
	}
	/// Adds the CHECKSIG operation to the script
	fn op_checksig(&mut self) -> &mut Self {
		self.0.push(0xAC);
		self
	}
	/// Serializes the script by returning a single byte for the length
	/// of the script and then the script itself
	fn serialize(&self) -> Vec<u8> {
		let mut result = vec![self.0.len() as u8];
		result.append(&mut self.0.clone());
		result
	}
}

// Derives a taproot address from a validator public key and a salt
pub fn derive_btc_ingress_address(pubkey_x: [u8; 32], salt: u32) -> String {
	let mut script = BitcoinScript::default();
	script.push_uint(salt).op_drop().push_32bytes(pubkey_x).op_checksig();
	let leafhash =
		sha2_256(&[TAPLEAF_HASH, TAPLEAF_HASH, &[0xC0_u8], &script.serialize()].concat());
	let tweakhash =
		sha2_256(&[TAPTWEAK_HASH, TAPTWEAK_HASH, &INTERNAL_PUBKEY[1..33], &leafhash].concat());
	let mut tweaked = PublicKey::parse_compressed(INTERNAL_PUBKEY.try_into().unwrap()).unwrap();
	_ = tweaked.tweak_add_assign(&SecretKey::parse(&tweakhash).unwrap());
	let segwit_version = u5::try_from_u8(1).unwrap();
	let mut payload = vec![segwit_version];
	payload.append(&mut tweaked.serialize_compressed()[1..33].as_ref().to_base32());
	bech32::encode("bc", &mut payload, Variant::Bech32m).unwrap()
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
	let mut script = BitcoinScript::default();
	script
		.push_uint(0)
		.op_drop()
		.push_32bytes(hex_literal::hex!(
			"2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"
		))
		.op_checksig();
	assert_eq!(
		script.serialize(),
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
		let mut script = BitcoinScript::default();
		script.push_uint(x.0);
		assert_eq!(script.0, x.1);
	}
}
