use super::*;
use crate::{ChainEnvironment, ChainflipEnvironment, DepositChannel};
use cf_primitives::ChannelId;
use frame_support::sp_runtime::DispatchError;
use sp_std::convert::TryInto;

const INTERNAL_PUBKEY: &[u8] =
	&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BitcoinDepositChannel {
	pub pubkey_x: [u8; 32],
	pub salt: u32,
	script_pubkey: ScriptPubkey,
	pub tapleaf_hash: Hash,
	unlock_script: BitcoinScript,
	leaf_version: u8,
}

fn unlock_script(pubkey_x: [u8; 32], salt: u32) -> BitcoinScript {
	BitcoinScript::new(&[
		BitcoinOp::PushUint { value: salt },
		BitcoinOp::Drop,
		BitcoinOp::PushArray32 { bytes: pubkey_x },
		BitcoinOp::CheckSig,
	])
}

impl BitcoinDepositChannel {
	pub fn new(pubkey_x: [u8; 32], salt: u32) -> Self {
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
		let script_pubkey = ScriptPubkey::Taproot(tweaked_pubkey_bytes[1..].as_array());
		let leaf_version = if tweaked_pubkey_bytes[0] == 2 { 0xC0 } else { 0xC1 };
		Self { pubkey_x, salt, script_pubkey, tapleaf_hash, unlock_script, leaf_version }
	}

	pub fn script_pubkey(&self) -> ScriptPubkey {
		self.script_pubkey.clone()
	}
}

impl DepositChannel<Bitcoin> for BitcoinDepositChannel {
	type Deposit = Utxo;

	fn generate_new<E: ChainflipEnvironment>(
		channel_id: cf_primitives::ChannelId,
		_asset: <Bitcoin as Chain>::ChainAsset,
	) -> Result<Self, DispatchError> {
		<E as ChainEnvironment<(), super::AggKey>>::lookup(())
			.ok_or(DispatchError::Other("Unable to look up bitcoin aggkey"))
			.and_then(|agg_key| {
				Ok(Self::new(
					agg_key.current,
					channel_id
						.try_into()
						.map_err(|_| DispatchError::Other("No more addresses available!"))?,
				))
			})
	}

	fn channel_id(&self) -> ChannelId {
		self.salt as ChannelId
	}

	fn address(&self) -> &<Bitcoin as Chain>::ChainAccount {
		&self.script_pubkey
	}

	fn fetch_params(&self, utxo: Self::Deposit) -> <Bitcoin as Chain>::FetchParams {
		BitcoinFetchParams { utxo, deposit_address: self.clone() }
	}
}

impl SerializeBtc for BitcoinDepositChannel {
	fn btc_encode_to(&self, buf: &mut Vec<u8>) {
		self.unlock_script.btc_encode_to(buf);
		// Length of tweaked pubkey + leaf version
		buf.push(33u8);
		buf.push(self.leaf_version);
		buf.extend_from_slice(&INTERNAL_PUBKEY[1..33]);
	}

	fn size(&self) -> usize {
		self.unlock_script.size() + 1 + 1 + 32
	}
}

#[test]
fn test_btc_derive_deposit_address() {
	assert_eq!(
		BitcoinDepositChannel::new(
			hex_literal::hex!("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"),
			0
		)
		.script_pubkey()
		.to_address(&BitcoinNetwork::Mainnet),
		"bc1p4syuuy97f96lfah764w33ru9v5u3uk8n8jk9xsq684xfl8sxu82sdcvdcx"
	);
	assert_eq!(
		BitcoinDepositChannel::new(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			15
		)
		.script_pubkey()
		.to_address(&BitcoinNetwork::Mainnet),
		"bc1phgs87wzfdqp9amtyc6darrhk3sm38tpf9a39mgjycthcet7vxl3qktqz86"
	);
	assert_eq!(
		BitcoinDepositChannel::new(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			50
		)
		.script_pubkey()
		.to_address(&BitcoinNetwork::Mainnet),
		"bc1p2uf6vzdzmv0u7wyfnljnrctr5qr6hy6mmzyjpr6z7x8yt39gppfq3a54c9"
	);
	assert_eq!(
		BitcoinDepositChannel::new(
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
			CHANGE_ADDRESS_SALT
		)
		.btc_serialize(),
		hex_literal::hex!(
			"240075202E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105AC"
		)
	);
}
