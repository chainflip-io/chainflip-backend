use super::*;
use base58::ToBase58;
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

pub fn derive_btc_deposit_bitcoin_script(pubkey_x: [u8; 32], salt: u32) -> BitcoinScript {
	BitcoinScript::default()
		.push_uint(SEGWIT_VERSION as u32)
		.push_bytes(tweaked_pubkey(pubkey_x, salt).serialize_compressed().into_iter().skip(1))
}

pub fn derive_btc_address_from_script(
	bitcoin_script: BitcoinScript,
	network: BitcoinNetwork,
) -> String {
	const CHECKSUM_LENGTH: usize = 4;
	match bitcoin_script.data[0] {
		0x00 => bech32::encode(
			network.bech32_and_bech32m_address_hrp(),
			itertools::chain!(
				iter::once(u5::try_from_u8(0u8).unwrap()),
				(&bitcoin_script.data[2..]).to_base32()
			)
			.collect::<Vec<_>>(),
			Variant::Bech32,
		)
		.unwrap(),
		0x51..=0x60 => bech32::encode(
			network.bech32_and_bech32m_address_hrp(),
			itertools::chain!(
				iter::once(u5::try_from_u8(bitcoin_script.data[0] - 0x50).unwrap()),
				(&bitcoin_script.data[2..]).to_base32()
			)
			.collect::<Vec<_>>(),
			Variant::Bech32m,
		)
		.unwrap(),
		0xa9 => {
			let mut payload = Vec::<u8>::default();
			payload.push(network.p2sh_address_version());
			payload.extend(&bitcoin_script.data[2..bitcoin_script.data.len().saturating_sub(1)]);
			payload.extend(&sha2_256(&sha2_256(&payload))[..CHECKSUM_LENGTH]);
			payload.to_base58()
		},
		0x76 => {
			let mut payload = Vec::<u8>::default();
			payload.push(network.p2pkh_address_version());
			payload.extend(&bitcoin_script.data[3..bitcoin_script.data.len().saturating_sub(2)]);
			payload.extend(&sha2_256(&sha2_256(&payload))[..CHECKSUM_LENGTH]);
			payload.to_base58()
		},
		_ => "".into(),
	}
}

#[test]
fn test_btc_derive_deposit_address() {
	assert_eq!(
		derive_btc_address_from_script(
			BitcoinScript {
				data: hex_literal::hex!("76a914de89566b3cda1c4b7ef85db035d747eaad0d408188ac")
					.into()
			},
			BitcoinNetwork::Regtest
		),
		"n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX"
	);
	assert_eq!(
		derive_btc_address_from_script(
			BitcoinScript {
				data: hex_literal::hex!("a914730d0dabce13a92834c990e24a5b6a1ffb72e9ef87").into()
			},
			BitcoinNetwork::Regtest
		),
		"2N3jZLJyhrHxG54nRRUszvbKHifGT4xGi2n"
	);
	assert_eq!(
		derive_btc_address_from_script(
			BitcoinScript {
				data: hex_literal::hex!("0014de89566b3cda1c4b7ef85db035d747eaad0d4081").into()
			},
			BitcoinNetwork::Regtest
		),
		"bcrt1qm6y4v6eumgwyklhctkcrt468a2ks6sypxrkghx"
	);
	assert_eq!(
		derive_btc_address_from_script(
			BitcoinScript {
				data: hex_literal::hex!(
					"00207d2d92da7b87a9fd6c9fb71e86de597953e9dc3e17bddde498af8197fbe22d25"
				)
				.into()
			},
			BitcoinNetwork::Regtest
		),
		"bcrt1q05ke9knms75l6mylku0gdhje09f7nhp7z77ameyc47qe07lz95jsldtm4e"
	);
	assert_eq!(
		derive_btc_address_from_script(
			derive_btc_deposit_bitcoin_script(
				hex_literal::hex!(
					"2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105"
				),
				0
			),
			BitcoinNetwork::Mainnet,
		),
		"bc1p4syuuy97f96lfah764w33ru9v5u3uk8n8jk9xsq684xfl8sxu82sdcvdcx"
	);
	assert_eq!(
		derive_btc_address_from_script(
			derive_btc_deposit_bitcoin_script(
				hex_literal::hex!(
					"FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"
				),
				15
			),
			BitcoinNetwork::Mainnet,
		),
		"bc1phgs87wzfdqp9amtyc6darrhk3sm38tpf9a39mgjycthcet7vxl3qktqz86"
	);
	assert_eq!(
		derive_btc_address_from_script(
			derive_btc_deposit_bitcoin_script(
				hex_literal::hex!(
					"FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"
				),
				50
			),
			BitcoinNetwork::Mainnet,
		),
		"bc1p2uf6vzdzmv0u7wyfnljnrctr5qr6hy6mmzyjpr6z7x8yt39gppfq3a54c9"
	);
	assert_eq!(
		derive_btc_address_from_script(
			derive_btc_deposit_bitcoin_script(
				hex_literal::hex!(
					"FEDBDC04F4666AF03167E2EF5FA5405BB012BC62A3B3180088E63972BD06EAD8"
				),
				123456789
			),
			BitcoinNetwork::Mainnet,
		),
		"bc1p8ea6zrds8q5mke8l6rlrluyle82xdr3sx4dk73r78l859gjfpsrq6gq3ev"
	);
}
