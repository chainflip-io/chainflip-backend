use super::*;
use sp_std::iter;

pub fn tweaked_pubkey(pubkey_x: [u8; 32], salt: u32) -> PublicKey {
	// SHA256("TapTweak")
	const TAPTWEAK_HASH: &[u8] =
		&hex_literal::hex!("e80fe1639c9ca050e3af1b39c143c63e429cbceb15d940fbb5c5a1f4af57c5e9");
	let leafhash = get_tapleaf_hash(pubkey_x, salt);
	let tweakhash =
		sha2_256(&[TAPTWEAK_HASH, TAPTWEAK_HASH, &INTERNAL_PUBKEY[1..33], &leafhash].concat());
	let mut tweaked = PublicKey::parse_compressed(INTERNAL_PUBKEY.try_into().unwrap()).unwrap();
	let _result = tweaked.tweak_add_assign(&SecretKey::parse(&tweakhash).unwrap());
	tweaked
}

// Derives a taproot address from a validator public key and a salt
pub fn derive_btc_ingress_address(
	pubkey_x: [u8; 32],
	salt: u32,
	btc_net: BitcoinNetwork,
) -> String {
	let tweaked = tweaked_pubkey(pubkey_x, salt).serialize_compressed()[1..33].to_vec();
	let segwit_version = u5::try_from_u8(SEGWIT_VERSION).unwrap();
	let mut payload = vec![segwit_version];
	payload.extend(tweaked.to_base32());
	let payload =
		itertools::chain!(iter::once(segwit_version), tweaked.to_base32()).collect::<Vec<_>>();
	bech32::encode(btc_net.bech32_pkh_address_prefix(), payload, Variant::Bech32m).unwrap()
}

#[test]
fn test_btc_derive_ingress_address() {
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"),
			0,
			BitcoinNetwork::Mainnet,
		),
		"bc1p4syuuy97f96lfah764w33ru9v5u3uk8n8jk9xsq684xfl8sxu82sdcvdcx"
	);
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			15,
			BitcoinNetwork::Mainnet,
		),
		"bc1phgs87wzfdqp9amtyc6darrhk3sm38tpf9a39mgjycthcet7vxl3qktqz86"
	);
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			50,
			BitcoinNetwork::Mainnet,
		),
		"bc1p2uf6vzdzmv0u7wyfnljnrctr5qr6hy6mmzyjpr6z7x8yt39gppfq3a54c9"
	);
	assert_eq!(
		derive_btc_ingress_address(
			hex_literal::hex!("FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"),
			123456789,
			BitcoinNetwork::Mainnet,
		),
		"bc1p8ea6zrds8q5mke8l6rlrluyle82xdr3sx4dk73r78l859gjfpsrq6gq3ev"
	);
}
