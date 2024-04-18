use crate::ChannelLifecycleHooks;

use super::*;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct TapscriptPath {
	pub salt: u32,
	pub tweaked_pubkey_bytes: [u8; 33],
	pub tapleaf_hash: [u8; 32],
	pub unlock_script: BitcoinScript,
}

/// The leaf version depends on the evenness of the tweaked pubkey.
impl TapscriptPath {
	pub fn leaf_version(&self) -> u8 {
		if self.tweaked_pubkey_bytes[0] == 2 {
			0xC0
		} else {
			0xC1
		}
	}
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct DepositAddress {
	pub pubkey_x: [u8; 32],
	pub script_path: Option<TapscriptPath>,
}

fn unlock_script(pubkey_x: [u8; 32], salt: u32) -> BitcoinScript {
	BitcoinScript::new(&[
		BitcoinOp::PushUint { value: salt },
		BitcoinOp::Drop,
		BitcoinOp::PushArray32 { bytes: pubkey_x },
		BitcoinOp::CheckSig,
	])
}

impl DepositAddress {
	pub fn new(pubkey_x: [u8; 32], salt: u32) -> Self {
		// All change goes back into the vault (i.e. salt = 0), but vault UTXOs can
		// be spent via the taproot key path to save gas
		if salt == CHANGE_ADDRESS_SALT {
			return Self { pubkey_x, script_path: None }
		}
		let unlock_script = unlock_script(pubkey_x, salt);
		let tapleaf_hash = {
			// SHA256("TapLeaf")
			const TAPLEAF_HASH: &[u8] = &hex_literal::hex!(
				"aeea8fdc4208983105734b58081d1e2638d35f1cb54008d4d357ca03be78e9ee"
			);
			const LEAF_VERSION: u8 = 0xC0;
			sha2_256(
				&[TAPLEAF_HASH, TAPLEAF_HASH, &[LEAF_VERSION], &unlock_script.btc_serialize()]
					.concat(),
			)
		};
		let tweaked_pubkey_bytes = {
			// SHA256("TapTweak")
			const TAPTWEAK_HASH: &[u8] = &hex_literal::hex!(
				"e80fe1639c9ca050e3af1b39c143c63e429cbceb15d940fbb5c5a1f4af57c5e9"
			);
			let tweak_hash = sha2_256(
				&[TAPTWEAK_HASH, TAPTWEAK_HASH, &INTERNAL_PUBKEY[1..33], &tapleaf_hash[..]]
					.concat(),
			);
			let mut tweaked =
				PublicKey::parse_compressed(INTERNAL_PUBKEY.try_into().unwrap()).unwrap();
			let _result = tweaked.tweak_add_assign(&SecretKey::parse(&tweak_hash).unwrap());
			tweaked.serialize_compressed()
		};
		Self {
			pubkey_x,
			script_path: Some(TapscriptPath {
				salt,
				tweaked_pubkey_bytes,
				tapleaf_hash,
				unlock_script,
			}),
		}
	}

	pub fn script_pubkey(&self) -> ScriptPubkey {
		let pubkey = self
			.script_path
			.clone()
			.map_or(self.pubkey_x, |script_path| script_path.tweaked_pubkey_bytes[1..].as_array());
		ScriptPubkey::Taproot(pubkey)
	}
}

impl ChannelLifecycleHooks for DepositAddress {
	// Default implementations are fine.
}

#[test]
fn test_btc_derive_deposit_address() {
	assert_eq!(
		DepositAddress::new(
			hex_literal::hex!("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"),
			0
		)
		.script_pubkey()
		.to_address(&BitcoinNetwork::Mainnet),
		"bc1p96yhxaszqgtu3cu95v9hfd6c9yuxxpyl5e4rl5thuqftqas9jyzsdersag"
	);
	assert_eq!(
		DepositAddress::new(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			15
		)
		.script_pubkey()
		.to_address(&BitcoinNetwork::Mainnet),
		"bc1phgs87wzfdqp9amtyc6darrhk3sm38tpf9a39mgjycthcet7vxl3qktqz86"
	);
	assert_eq!(
		DepositAddress::new(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			50
		)
		.script_pubkey()
		.to_address(&BitcoinNetwork::Mainnet),
		"bc1p2uf6vzdzmv0u7wyfnljnrctr5qr6hy6mmzyjpr6z7x8yt39gppfq3a54c9"
	);
	assert_eq!(
		DepositAddress::new(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			123456789
		)
		.script_pubkey()
		.to_address(&BitcoinNetwork::Mainnet),
		"bc1p8ea6zrds8q5mke8l6rlrluyle82xdr3sx4dk73r78l859gjfpsrq6gq3ev"
	);
}

#[test]
fn test_build_script() {
	assert_eq!(
		unlock_script(
			hex_literal::hex!("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"),
			1
		)
		.btc_serialize(),
		hex_literal::hex!(
			"245175202E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105AC"
		)
	);
}
