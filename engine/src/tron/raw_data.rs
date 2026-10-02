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

//! Decoding and validation of the `raw_data` protobuf that a Tron RPC endpoint builds for us.
//!
//! The broadcaster asks the endpoint to assemble the unsigned transaction and then signs the bytes
//! it gets back, so without this check a compromised endpoint would choose what the broadcaster key
//! authorises. Every field Tron's protocol defines is declared in [`wire`] and either checked or
//! explicitly ignored, so the endpoint cannot smuggle anything past us in a field we forgot to look
//! at.

use anyhow::{anyhow, ensure, Context};
use cf_chains::tron::{TronAddress, TronTransaction};
use prost::Message;

/// `Transaction.Contract.ContractType` discriminant for `TriggerSmartContract`.
const TRIGGER_SMART_CONTRACT_TYPE: i32 = 31;
const TRIGGER_SMART_CONTRACT_TYPE_URL: &str = "type.googleapis.com/protocol.TriggerSmartContract";

/// The `protocol.TriggerSmartContract` message, as carried in `Transaction.raw.contract`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerSmartContract {
	pub owner_address: TronAddress,
	pub contract_address: TronAddress,
	pub call_value: i64,
	pub data: Vec<u8>,
	pub call_token_value: i64,
	pub token_id: i64,
}

/// The subset of `Transaction.raw` that determines what the transaction does.
///
/// `ref_block_*`, `expiration` and `timestamp` are decoded but discarded: they are only relevant
/// when actually submitting the transaction to the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TronRawData {
	pub fee_limit: i64,
	pub contract: TriggerSmartContract,
}

/// Wire-level mirrors of the Tron protobuf messages, declaring every field the protocol defines.
/// Field numbers come from `core/Tron.proto` and `core/contract/smart_contract.proto`.
mod wire {
	#[derive(Clone, PartialEq, prost::Message)]
	pub struct RawData {
		#[prost(bytes = "vec", tag = "1")]
		pub ref_block_bytes: Vec<u8>,
		#[prost(int64, tag = "3")]
		pub ref_block_num: i64,
		#[prost(bytes = "vec", tag = "4")]
		pub ref_block_hash: Vec<u8>,
		#[prost(int64, tag = "8")]
		pub expiration: i64,
		/// `repeated authority`, decoded opaquely: we only care that there are none.
		#[prost(bytes = "vec", repeated, tag = "9")]
		pub auths: Vec<Vec<u8>>,
		#[prost(bytes = "vec", tag = "10")]
		pub data: Vec<u8>,
		#[prost(message, repeated, tag = "11")]
		pub contract: Vec<Contract>,
		#[prost(bytes = "vec", tag = "12")]
		pub scripts: Vec<u8>,
		#[prost(int64, tag = "14")]
		pub timestamp: i64,
		#[prost(int64, tag = "18")]
		pub fee_limit: i64,
	}

	#[derive(Clone, PartialEq, prost::Message)]
	pub struct Contract {
		#[prost(int32, tag = "1")]
		pub r#type: i32,
		#[prost(message, optional, tag = "2")]
		pub parameter: Option<Any>,
		#[prost(bytes = "vec", tag = "3")]
		pub provider: Vec<u8>,
		#[prost(bytes = "vec", tag = "4")]
		pub contract_name: Vec<u8>,
		#[prost(int32, tag = "5")]
		pub permission_id: i32,
	}

	#[derive(Clone, PartialEq, prost::Message)]
	pub struct Any {
		#[prost(string, tag = "1")]
		pub type_url: String,
		#[prost(bytes = "vec", tag = "2")]
		pub value: Vec<u8>,
	}

	#[derive(Clone, PartialEq, prost::Message)]
	pub struct TriggerSmartContract {
		#[prost(bytes = "vec", tag = "1")]
		pub owner_address: Vec<u8>,
		#[prost(bytes = "vec", tag = "2")]
		pub contract_address: Vec<u8>,
		#[prost(int64, tag = "3")]
		pub call_value: i64,
		#[prost(bytes = "vec", tag = "4")]
		pub data: Vec<u8>,
		#[prost(int64, tag = "5")]
		pub call_token_value: i64,
		#[prost(int64, tag = "6")]
		pub token_id: i64,
	}
}

fn decode_address(bytes: &[u8]) -> anyhow::Result<TronAddress> {
	TronAddress::try_from(bytes.to_vec()).map_err(|e| anyhow!("{e}"))
}

impl TronRawData {
	pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
		let raw_data = wire::RawData::decode(bytes).context("Failed to decode raw_data")?;

		// We never ask for any of these, so anything here was added by the endpoint. Note that
		// `ref_block_*`, `expiration` and `timestamp` are ignored on purpose.
		ensure!(raw_data.auths.is_empty(), "unexpected auths in raw_data");
		ensure!(raw_data.data.is_empty(), "unexpected data memo in raw_data");
		ensure!(raw_data.scripts.is_empty(), "unexpected scripts in raw_data");

		let [contract] = <[_; 1]>::try_from(raw_data.contract).map_err(|contracts: Vec<_>| {
			anyhow!("expected exactly one contract operation, got {}", contracts.len())
		})?;

		ensure!(
			contract.r#type == TRIGGER_SMART_CONTRACT_TYPE,
			"unexpected contract type: {}",
			contract.r#type
		);
		ensure!(contract.provider.is_empty(), "unexpected provider in contract");
		ensure!(contract.contract_name.is_empty(), "unexpected contract_name in contract");
		// A non-zero permission id would sign under a different account permission.
		ensure!(
			contract.permission_id == 0,
			"unexpected permission_id: {}",
			contract.permission_id
		);

		let parameter =
			contract.parameter.ok_or_else(|| anyhow!("Contract is missing parameter"))?;
		ensure!(
			parameter.type_url == TRIGGER_SMART_CONTRACT_TYPE_URL,
			"unexpected contract type_url: {}",
			parameter.type_url
		);

		let trigger = wire::TriggerSmartContract::decode(parameter.value.as_slice())
			.context("Failed to decode TriggerSmartContract")?;

		Ok(Self {
			fee_limit: raw_data.fee_limit,
			contract: TriggerSmartContract {
				owner_address: decode_address(&trigger.owner_address)?,
				contract_address: decode_address(&trigger.contract_address)?,
				call_value: trigger.call_value,
				data: trigger.data,
				call_token_value: trigger.call_token_value,
				token_id: trigger.token_id,
			},
		})
	}
}

/// Check that the transaction the endpoint built is the one the State Chain asked for.
pub fn validate_raw_data(
	raw_data_bytes: &[u8],
	transaction: &TronTransaction,
	signer: TronAddress,
	expected_fee_limit: i64,
) -> anyhow::Result<()> {
	// `value` is never sent to the RPC, so a non-zero one would be silently dropped rather than
	// broadcast. Until it is plumbed through, refuse to sign instead.
	ensure!(
		transaction.value.is_zero(),
		"TronTransaction.value is not supported: {}",
		transaction.value
	);

	let TronRawData { fee_limit, contract } = TronRawData::decode(raw_data_bytes)?;

	ensure!(
		fee_limit == expected_fee_limit,
		"fee_limit mismatch: expected {expected_fee_limit}, got {fee_limit}"
	);
	ensure!(
		contract.owner_address == signer,
		"owner_address mismatch: expected {signer}, got {}",
		contract.owner_address
	);
	let expected_contract = TronAddress::from_evm_address(transaction.contract);
	ensure!(
		contract.contract_address == expected_contract,
		"contract_address mismatch: expected {expected_contract}, got {}",
		contract.contract_address
	);
	ensure!(
		contract.data == transaction.data,
		"calldata mismatch: expected {}, got {}",
		hex::encode(&transaction.data),
		hex::encode(&contract.data)
	);
	ensure!(contract.call_value == 0, "unexpected call_value: {}", contract.call_value);
	ensure!(
		contract.call_token_value == 0 && contract.token_id == 0,
		"unexpected TRC10 transfer: {} of token {}",
		contract.call_token_value,
		contract.token_id
	);

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use cf_utilities::{assert_err, assert_ok};
	use ethers::types::H160;

	const OWNER: H160 = H160([0x11; 20]);
	const CONTRACT: H160 = H160([0x22; 20]);
	const ATTACKER: H160 = H160([0x66; 20]);
	const FEE_LIMIT: i64 = 1_000_000;

	fn calldata() -> Vec<u8> {
		hex::decode(concat!(
			"a9059cbb",
			"0000000000000000000000004115208ef33a926919ed270e2fa61367b2da3753",
			"0000000000000000000000000000000000000000000000000000000000000032",
		))
		.unwrap()
	}

	fn encode_varint(mut value: u64) -> Vec<u8> {
		let mut encoded = Vec::new();
		loop {
			let byte = (value & 0x7f) as u8;
			value >>= 7;
			if value == 0 {
				encoded.push(byte);
				return encoded;
			}
			encoded.push(byte | 0x80);
		}
	}

	fn varint_field(field_number: u32, value: i64) -> Vec<u8> {
		let mut encoded = encode_varint(u64::from(field_number) << 3);
		encoded.extend(encode_varint(value as u64));
		encoded
	}

	fn bytes_field(field_number: u32, value: &[u8]) -> Vec<u8> {
		let mut encoded = encode_varint((u64::from(field_number) << 3) | 2);
		encoded.extend(encode_varint(value.len() as u64));
		encoded.extend_from_slice(value);
		encoded
	}

	fn tron_address_bytes(address: H160) -> Vec<u8> {
		let mut encoded = vec![0x41];
		encoded.extend_from_slice(address.as_bytes());
		encoded
	}

	/// Mirrors what a Tron node returns from `/wallet/triggersmartcontract`.
	struct RawDataBuilder {
		owner: H160,
		contract: H160,
		call_value: i64,
		data: Vec<u8>,
		fee_limit: i64,
		contract_type: i32,
		type_url: String,
		contract_count: usize,
		extra_raw_data_fields: Vec<u8>,
		extra_contract_fields: Vec<u8>,
		extra_trigger_fields: Vec<u8>,
	}

	impl Default for RawDataBuilder {
		fn default() -> Self {
			Self {
				owner: OWNER,
				contract: CONTRACT,
				call_value: 0,
				data: calldata(),
				fee_limit: FEE_LIMIT,
				contract_type: TRIGGER_SMART_CONTRACT_TYPE,
				type_url: TRIGGER_SMART_CONTRACT_TYPE_URL.to_string(),
				contract_count: 1,
				extra_raw_data_fields: Vec::new(),
				extra_contract_fields: Vec::new(),
				extra_trigger_fields: Vec::new(),
			}
		}
	}

	impl RawDataBuilder {
		fn build(self) -> Vec<u8> {
			let mut trigger = bytes_field(1, &tron_address_bytes(self.owner));
			trigger.extend(bytes_field(2, &tron_address_bytes(self.contract)));
			if self.call_value != 0 {
				trigger.extend(varint_field(3, self.call_value));
			}
			trigger.extend(bytes_field(4, &self.data));
			trigger.extend(self.extra_trigger_fields);

			let mut any = bytes_field(1, self.type_url.as_bytes());
			any.extend(bytes_field(2, &trigger));

			let mut contract = varint_field(1, self.contract_type as i64);
			contract.extend(bytes_field(2, &any));
			contract.extend(self.extra_contract_fields);

			// Field order follows the ascending field numbers a node emits.
			let mut raw_data = bytes_field(1, &[0x12, 0x34]);
			raw_data.extend(bytes_field(4, &[0xab; 8]));
			raw_data.extend(varint_field(8, 1_700_000_060_000));
			for _ in 0..self.contract_count {
				raw_data.extend(bytes_field(11, &contract));
			}
			raw_data.extend(varint_field(14, 1_700_000_000_000));
			raw_data.extend(varint_field(18, self.fee_limit));
			raw_data.extend(self.extra_raw_data_fields);
			raw_data
		}
	}

	fn request() -> TronTransaction {
		TronTransaction {
			contract: CONTRACT,
			function_selector: b"transfer(address,uint256)".to_vec(),
			data: calldata(),
			fee_limit: Some(FEE_LIMIT as u64),
			value: Default::default(),
		}
	}

	fn validate(raw_data: Vec<u8>) -> anyhow::Result<()> {
		validate_raw_data(&raw_data, &request(), TronAddress::from_evm_address(OWNER), FEE_LIMIT)
	}

	/// `RawDataBuilder::default()` encoded by tronweb's generated `Transaction.raw` protobuf
	/// (`core/Tron.proto`). Anchors the hand-written encoder below — and therefore every other
	/// test — to the encoding a real Tron node produces.
	const TRON_ENCODED_RAW_DATA: &str = "0a0212342208abababababababab40e0a499ffbc315aae01081f12a9010a31747970652e676f6f676c65617069732e636f6d2f70726f746f636f6c2e54726967676572536d617274436f6e747261637412740a1541111111111111111111111111111111111111111112154122222222222222222222222222222222222222222244a9059cbb0000000000000000000000004115208ef33a926919ed270e2fa61367b2da375300000000000000000000000000000000000000000000000000000000000000327080d095ffbc319001c0843d";

	#[test]
	fn builder_matches_tron_protobuf_encoding() {
		assert_eq!(hex::encode(RawDataBuilder::default().build()), TRON_ENCODED_RAW_DATA);
	}

	#[test]
	fn accepts_the_transaction_we_asked_for() {
		assert_ok!(validate(RawDataBuilder::default().build()));
	}

	#[test]
	fn decodes_the_contract_call() {
		assert_eq!(
			assert_ok!(TronRawData::decode(&RawDataBuilder::default().build())),
			TronRawData {
				fee_limit: FEE_LIMIT,
				contract: TriggerSmartContract {
					owner_address: TronAddress::from_evm_address(OWNER),
					contract_address: TronAddress::from_evm_address(CONTRACT),
					call_value: 0,
					data: calldata(),
					call_token_value: 0,
					token_id: 0,
				},
			}
		);
	}

	#[test]
	fn rejects_substituted_target_contract() {
		assert_err!(validate(RawDataBuilder { contract: ATTACKER, ..Default::default() }.build()));
	}

	#[test]
	fn rejects_substituted_owner() {
		assert_err!(validate(RawDataBuilder { owner: ATTACKER, ..Default::default() }.build()));
	}

	#[test]
	fn rejects_substituted_calldata() {
		let mut tampered = calldata();
		tampered[36..56].copy_from_slice(ATTACKER.as_bytes());
		assert_err!(validate(RawDataBuilder { data: tampered, ..Default::default() }.build()));
	}

	#[test]
	fn rejects_truncated_calldata() {
		assert_err!(validate(
			RawDataBuilder { data: calldata()[..4].to_vec(), ..Default::default() }.build()
		));
	}

	#[test]
	fn rejects_non_zero_call_value() {
		assert_err!(validate(RawDataBuilder { call_value: 1, ..Default::default() }.build()));
	}

	#[test]
	fn rejects_trc10_token_transfer() {
		let mut extra_trigger_fields = varint_field(5, 1);
		extra_trigger_fields.extend(varint_field(6, 1_000_005));
		assert_err!(validate(
			RawDataBuilder { extra_trigger_fields, ..Default::default() }.build()
		));
	}

	#[test]
	fn rejects_fee_limit_mismatch() {
		assert_err!(validate(
			RawDataBuilder { fee_limit: FEE_LIMIT + 1, ..Default::default() }.build()
		));
	}

	#[test]
	fn rejects_contract_type_other_than_trigger_smart_contract() {
		// 1 == TransferContract, a plain TRX transfer.
		assert_err!(validate(RawDataBuilder { contract_type: 1, ..Default::default() }.build()));
	}

	#[test]
	fn rejects_mismatched_any_type_url() {
		assert_err!(validate(
			RawDataBuilder {
				type_url: "type.googleapis.com/protocol.TransferContract".to_string(),
				..Default::default()
			}
			.build()
		));
	}

	#[test]
	fn rejects_batched_contract_calls() {
		assert_err!(validate(RawDataBuilder { contract_count: 2, ..Default::default() }.build()));
	}

	#[test]
	fn rejects_missing_contract_call() {
		assert_err!(validate(RawDataBuilder { contract_count: 0, ..Default::default() }.build()));
	}

	#[test]
	fn rejects_unrecognised_raw_data_field() {
		// A `data` memo (field 10) is bandwidth we never asked to pay for.
		assert_err!(validate(
			RawDataBuilder {
				extra_raw_data_fields: bytes_field(10, b"memo"),
				..Default::default()
			}
			.build()
		));
	}

	#[test]
	fn rejects_unrecognised_contract_field() {
		// `Permission_id` (field 5) would sign under a different account permission.
		assert_err!(validate(
			RawDataBuilder { extra_contract_fields: varint_field(5, 2), ..Default::default() }
				.build()
		));
	}

	#[test]
	fn rejects_trailing_bytes() {
		let mut raw_data = RawDataBuilder::default().build();
		raw_data.push(0xff);
		assert_err!(validate(raw_data));
	}

	#[test]
	fn rejects_truncated_raw_data() {
		let raw_data = RawDataBuilder::default().build();
		assert_err!(validate(raw_data[..raw_data.len() - 1].to_vec()));
	}

	#[test]
	fn rejects_request_with_non_zero_value() {
		// `value` is not plumbed through to the RPC, so it must not be silently dropped.
		assert_err!(validate_raw_data(
			&RawDataBuilder::default().build(),
			&TronTransaction { value: 1.into(), ..request() },
			TronAddress::from_evm_address(OWNER),
			FEE_LIMIT,
		));
	}
}
