// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use super::*;

use core::str::FromStr;

use cf_chains::{
	btc::{self, BitcoinTransactionData},
	dot::{EncodedPolkadotPayload, PolkadotAccountId, PolkadotTransactionData},
	evm::{self, Address, ParityBit, H256},
	sol::{
		sol_tx_core::{CompiledInstruction, MessageHeader},
		RawSolHash, SolPubkey, SolVersionedMessage, SolVersionedMessageV0, SolVersionedTransaction,
		SolanaTransactionData,
	},
};
use cf_primitives::AccountId;
use codec::Encode;

#[track_caller]
fn check_encoding(event: CfeEvent<AccountId>, expected: &str) {
	assert_eq!(hex::encode(event.encode()), expected);
}

#[test]
fn event_decoding() {
	let participants = BTreeSet::from([AccountId::from([1; 32]), AccountId::from([2; 32])]);

	// Signature requests
	{
		check_encoding(CfeEvent::EvmThresholdSignatureRequest(ThresholdSignatureRequest::<AccountId, _> {
					ceremony_id: 1,
					epoch_index: 2,
					key: evm::AggKey {
						pub_key_x: [
							5, 27, 14, 199, 91, 236, 221, 212, 98, 63, 41, 107, 38, 81, 55, 241,
							109, 184, 91, 13, 229, 185, 245, 14, 204, 220, 30, 110, 46, 30, 180,
							103,
						],
						pub_key_y_parity: ParityBit::Even,
					},
					signatories: participants.clone(),
					payload: H256::from_str(
						"dc24f5f2ca2d74483d546815943a90827265b99ca3f1e0e139053794b041acf9",
					)
					.unwrap(),
				}), "00010000000000000002000000051b0ec75becddd4623f296b265137f16db85b0de5b9f50eccdc1e6e2e1eb467010801010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202dc24f5f2ca2d74483d546815943a90827265b99ca3f1e0e139053794b041acf9");

		check_encoding(
				CfeEvent::BtcThresholdSignatureRequest(ThresholdSignatureRequest::<AccountId, _> {
					ceremony_id: 1,
					epoch_index: 2,
					key: btc::AggKey {
						previous: None,
						current: [
							37, 136, 41, 15, 101, 49, 148, 182, 235, 239, 4, 136, 14, 27, 42, 100,
							178, 8, 76, 169, 133, 233, 4, 250, 103, 170, 9, 100, 18, 186, 150, 210,
						],
					},
					signatories: participants.clone(),
					payload: vec![(
						btc::PreviousOrCurrent::Current,
						[
							37, 135, 41, 15, 101, 49, 148, 182, 235, 239, 4, 136, 14, 27, 42, 100,
							178, 8, 76, 169, 133, 233, 4, 250, 103, 170, 9, 100, 18, 186, 150, 210,
						],
					)],
				}),
				"02010000000000000002000000002588290f653194b6ebef04880e1b2a64b2084ca985e904fa67aa096412ba96d2080101010101010101010101010101010101010101010101010101010101010101020202020202020202020202020202020202020202020202020202020202020204012587290f653194b6ebef04880e1b2a64b2084ca985e904fa67aa096412ba96d2",
			);

		check_encoding(CfeEvent::DotThresholdSignatureRequest(ThresholdSignatureRequest::<AccountId, _> {
				ceremony_id: 1,
				epoch_index: 2,
				key: PolkadotAccountId::from_aliased([
					122, 146, 31, 46, 127, 138, 236, 28, 42, 166, 38, 120, 89, 213, 142, 162,
					118, 47, 222, 215, 18, 233, 250, 37, 211, 221, 198, 169, 58, 99, 229, 106,
				]),
				signatories: participants.clone(),
				payload: EncodedPolkadotPayload(vec![
					83, 0, 103, 101, 131, 6, 118, 36, 254, 171, 194, 92, 101, 225, 6, 183, 47,
					26, 177, 23, 110, 251, 101, 104, 16, 37, 5, 166, 230, 32, 125, 201,
				]),
			}), "010100000000000000020000007a921f2e7f8aec1c2aa6267859d58ea2762fded712e9fa25d3ddc6a93a63e56a0801010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202805300676583067624feabc25c65e106b72f1ab1176efb6568102505a6e6207dc9");

		check_encoding(CfeEvent::SolThresholdSignatureRequest(ThresholdSignatureRequest::<AccountId, _> {
				ceremony_id: 1,
				epoch_index: 2,
				key: [7u8;32].into(),
				signatories: participants.clone(),
				payload: SolVersionedMessage::V0(SolVersionedMessageV0{
					header: MessageHeader {
						num_readonly_signed_accounts: 1,
						num_readonly_unsigned_accounts: 1,
						num_required_signatures: 1,
					},
					account_keys: vec![SolPubkey([7u8;32])],
					recent_blockhash: RawSolHash([8u8; 32]),
					instructions: vec![CompiledInstruction {
						program_id_index:1,
						accounts: vec![0],
						data: vec![3,4,5,6]
					}],
					address_table_lookups: vec![] // TODO roy: add test case with ALT
				})
			}), "0d0100000000000000020000000707070707070707070707070707070707070707070707070707070707070707080101010101010101010101010101010101010101010101010101010101010101020202020202020202020202020202020202020202020202020202020202020201010101040707070707070707070707070707070707070707070707070707070707070707080808080808080808080808080808080808080808080808080808080808080804010400100304050600");
	}

	// Keygen requests
	{
		let keygen_request = KeygenRequest::<AccountId> {
			ceremony_id: 1,
			epoch_index: 2,
			participants: participants.clone(),
		};

		check_encoding(CfeEvent::EvmKeygenRequest(keygen_request.clone()), "030100000000000000020000000801010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202");
		check_encoding(CfeEvent::DotKeygenRequest(keygen_request.clone()), "040100000000000000020000000801010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202");
		check_encoding(CfeEvent::BtcKeygenRequest(keygen_request.clone()), "050100000000000000020000000801010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202");
		check_encoding(CfeEvent::SolKeygenRequest(keygen_request.clone()), "0e0100000000000000020000000801010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202");
	}

	// Handover request
	{
		check_encoding(CfeEvent::BtcKeyHandoverRequest(KeyHandoverRequest {
				ceremony_id: 5,
				from_epoch: 2,
				to_epoch: 3,
				key_to_share: btc::AggKey {
					previous: None,
					current: [
						37, 136, 41, 15, 101, 49, 148, 182, 235, 239, 4, 136, 14, 27, 42, 100, 178,
						8, 76, 169, 133, 233, 4, 250, 103, 170, 9, 100, 18, 186, 150, 210,
					],
				},
				sharing_participants: participants.clone(),
				receiving_participants: BTreeSet::from([
					AccountId::from([3; 32]),
					AccountId::from([4; 32]),
				]),
				new_key: btc::AggKey {
					previous: None,
					current: [
						87, 131, 102, 68, 121, 214, 207, 237, 173, 161, 171, 136, 250, 247, 52, 35,
						78, 2, 10, 152, 223, 83, 28, 43, 230, 122, 193, 71, 120, 194, 214, 229,
					],
				},
			}), "0605000000000000000200000003000000002588290f653194b6ebef04880e1b2a64b2084ca985e904fa67aa096412ba96d208010101010101010101010101010101010101010101010101010101010101010102020202020202020202020202020202020202020202020202020202020202020803030303030303030303030303030303030303030303030303030303030303030404040404040404040404040404040404040404040404040404040404040404005783664479d6cfedada1ab88faf734234e020a98df531c2be67ac14778c2d6e5");
	}

	// Tx broadcast requests
	{
		check_encoding(CfeEvent::EthTxBroadcastRequest(TxBroadcastRequest {
				broadcast_id: 1,
				nominee: AccountId::from([1; 32]),
				payload: evm::Transaction {
					chain_id: 10997,
					max_priority_fee_per_gas: Some(0.into()),
					max_fee_per_gas: Some(14.into()),
					gas_limit: None,
					contract: Address::from([
						161, 110, 2, 232, 123, 116, 84, 18, 110, 94, 16, 217, 87, 169, 39, 167,
						245, 181, 210, 190,
					]),
					value: 0.into(),
					data: vec![193, 196, 161, 89, 97, 109],
				},
			}), "07010000000101010101010101010101010101010101010101010101010101010101010101f52a000000000000010000000000000000000000000000000000000000000000000000000000000000010e0000000000000000000000000000000000000000000000000000000000000000a16e02e87b7454126e5e10d957a927a7f5b5d2be000000000000000000000000000000000000000000000000000000000000000018c1c4a159616d");

		check_encoding(CfeEvent::DotTxBroadcastRequest(TxBroadcastRequest {
				broadcast_id: 1,
				nominee: AccountId::from([1; 32]),
				payload: PolkadotTransactionData {
					encoded_extrinsic: vec![217, 7, 132, 0, 102, 145],
				},
			}), "0801000000010101010101010101010101010101010101010101010101010101010101010118d90784006691");

		check_encoding(CfeEvent::BtcTxBroadcastRequest(TxBroadcastRequest {
				broadcast_id: 1,
				nominee: AccountId::from([1; 32]),
				payload: BitcoinTransactionData { encoded_transaction: vec![2, 0, 1, 7, 23, 241] },
			}), "09010000000101010101010101010101010101010101010101010101010101010101010101180200010717f1");
		check_encoding(CfeEvent::SolTxBroadcastRequest(TxBroadcastRequest {
				broadcast_id: 1,
				nominee: AccountId::from([1; 32]),
				payload: SolanaTransactionData{ serialized_transaction: (SolVersionedTransaction {
					signatures: vec![[9u8; 64].into()],
					message: SolVersionedMessage::V0( SolVersionedMessageV0 {
						header: MessageHeader {
							num_readonly_signed_accounts: 2,
							num_readonly_unsigned_accounts: 2,
							num_required_signatures: 2,
						},
						account_keys: vec![SolPubkey([10u8;32])],
						recent_blockhash: RawSolHash([11u8; 32]),
						instructions: vec![CompiledInstruction {
							program_id_index:2,
							accounts: vec![1],
							data: vec![31,41,51,61]
						}],
						address_table_lookups: vec![] // TODO roy: add test case with ALT
					}),
				}).finalize_and_serialize().unwrap(),
				skip_preflight: false,
			}
			}), "0f0100000001010101010101010101010101010101010101010101010101010101010101014102010909090909090909090909090909090909090909090909090909090909090909090909090909090909090909090909090909090909090909090909090909090980020202010a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b01020101041f29333d0000");
	}

	// P2P registration/deregistration
	{
		let pubkey = Ed25519PublicKey::from_raw([
			80, 25, 187, 38, 192, 238, 214, 73, 246, 54, 234, 14, 139, 5, 161, 150, 28, 141, 138,
			160, 83, 158, 160, 81, 61, 241, 122, 38, 56, 123, 20, 87,
		]);

		check_encoding(CfeEvent::PeerIdRegistered {
				account_id: AccountId::from([1; 32]),
				pubkey,
				port: 3100,
				ip: 281472812449793,
			}, "0b01010101010101010101010101010101010101010101010101010101010101015019bb26c0eed649f636ea0e8b05a1961c8d8aa0539ea0513df17a26387b14571c0c0100007fffff00000000000000000000");

		check_encoding(CfeEvent::PeerIdDeregistered {
				account_id: AccountId::from([1; 32]), pubkey
			}, "0c01010101010101010101010101010101010101010101010101010101010101015019bb26c0eed649f636ea0e8b05a1961c8d8aa0539ea0513df17a26387b1457");
	}
}
