use cf_chains::sol::{
	sol_tx_core::{CompiledInstruction, MessageHeader},
	SolMessage, SolPubkey, SolSignature,
};

use cf_chains::sol::{SolHash, SolanaTransactionData};
use genesis::with_test_defaults;
use sp_runtime::AccountId32;

use frame_support::traits::UncheckedOnRuntimeUpgrade;
use pallet_cf_broadcast::BroadcastData;
use state_chain_runtime::{
	migrations::serialize_solana_broadcast::{self, old, SerializeSolanaBroadcastMigration},
	SolanaInstance,
};

use crate::*;

use cf_chains::sol::SolTransaction;

// Test data pulled from `state-chain/chains/src/sol/sol_tx_core.rs`
#[test]
fn test_migration() {
	with_test_defaults().build().execute_with(|| {
		let tx: SolTransaction = SolTransaction {
            signatures: vec![
                SolSignature(hex_literal::hex!(
                    "d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c"
                )),
            ],
            message: SolMessage {
                header: MessageHeader {
                    num_required_signatures: 1,
                    num_readonly_signed_accounts: 0,
                    num_readonly_unsigned_accounts: 8,
                },
                account_keys: vec![
                    SolPubkey(hex_literal::hex!(
                        "2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "22020a74fd97df45db96d2bbf4e485ccbec56945155ff8f668856be26c9de4a9"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "79c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "8cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adb"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "f53a2f4350451db5595a75e231519bc2758798f72550e57487722e7cbe954dbc"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "0000000000000000000000000000000000000000000000000000000000000000"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "0306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a40000000"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "06a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "0fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee87"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aa"
                    )),
                    SolPubkey(hex_literal::hex!(
                        "a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b"
                    )),
                ],
                recent_blockhash: SolHash(hex_literal::hex!(
                    "f7f02ac4729abaa97c01aa6526ba909c3bcb16c7f47c7e13dfdc5a1b15f647b4"
                ))
                .into(),
                instructions: vec![
                    CompiledInstruction {
                        program_id_index: 7,
                        accounts: hex_literal::hex!("030900").to_vec(),
                        data: hex_literal::hex!("04000000").to_vec(),
                    },
                    CompiledInstruction {
                        program_id_index: 8,
                        accounts: vec![],
                        data: hex_literal::hex!("030a00000000000000").to_vec(),
                    },
                    CompiledInstruction {
                        program_id_index: 8,
                        accounts: vec![],
                        data: hex_literal::hex!("0233620100").to_vec(),
                    },
                    CompiledInstruction {
                        program_id_index: 12,
                        accounts: hex_literal::hex!("0e00040507").to_vec(),
                        data: hex_literal::hex!("8e24658f6c59298c080000000100000000000000ff").to_vec(),
                    },
                    CompiledInstruction {
                        program_id_index: 12,
                        accounts: hex_literal::hex!("0e000d01020b0a0607").to_vec(),
                        data: hex_literal::hex!("494710642cb0c646080000000200000000000000ff06").to_vec(),
                    },
                ],
            },
        };

		old::AwaitingBroadcast::insert(
			22,
			old::SolanaBroadcastData {
				broadcast_id: 22,
				transaction_payload: tx,
				threshold_signature_payload: SolMessage::default(),
				transaction_out_id: SolSignature::default(),
				nominee: Some(AccountId32::from([11; 32])),
			},
		);

		let state = serialize_solana_broadcast::pre_upgrade_check().unwrap();
		SerializeSolanaBroadcastMigration::on_runtime_upgrade();
		serialize_solana_broadcast::post_upgrade_check(state).unwrap();

		let expected_serialized_tx = hex_literal::hex!("01d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c0100080f2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef222020a74fd97df45db96d2bbf4e485ccbec56945155ff8f668856be26c9de4a979c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f30368cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adb8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7ddf53a2f4350451db5595a75e231519bc2758798f72550e57487722e7cbe954dbc00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8772b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaa1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bf7f02ac4729abaa97c01aa6526ba909c3bcb16c7f47c7e13dfdc5a1b15f647b40507030309000404000000080009030a0000000000000008000502336201000c050e00040507158e24658f6c59298c080000000100000000000000ff0c090e000d01020b0a060716494710642cb0c646080000000200000000000000ff06").to_vec();

		let mut broadcast_iter =
			pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::iter();
		let (first_broadcast_id, first_broadcast_data) = broadcast_iter.next().unwrap();
		assert!(broadcast_iter.next().is_none());

		assert_eq!(first_broadcast_id, 22);
		assert_eq!(
			first_broadcast_data,
			BroadcastData {
				broadcast_id: 22,
				transaction_payload: SolanaTransactionData {
					serialized_transaction: expected_serialized_tx,
				},
				threshold_signature_payload: SolMessage::default(),
				transaction_out_id: SolSignature::default(),
				nominee: Some(AccountId32::from([11; 32])),
			}
		);
	});
}
