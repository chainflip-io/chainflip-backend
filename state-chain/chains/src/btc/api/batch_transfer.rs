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

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

use crate::btc::{AggKey, BitcoinCrypto, BitcoinOutput, BitcoinTransaction, Utxo};

use crate::{ApiCall, ChainCrypto};

use frame_support::sp_runtime::RuntimeDebug;

/// Represents all the arguments required to build the call to fetch assets for all given channel
/// ids.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BatchTransfer {
	pub bitcoin_transaction: BitcoinTransaction,
	pub change_utxo_key: [u8; 32],
}

impl BatchTransfer {
	pub fn new_unsigned(
		agg_key: &AggKey,
		change_utxo_key: [u8; 32],
		input_utxos: Vec<Utxo>,
		outputs: Vec<BitcoinOutput>,
	) -> Self {
		Self {
			bitcoin_transaction: BitcoinTransaction::create_new_unsigned(
				agg_key,
				input_utxos,
				outputs,
			),
			change_utxo_key,
		}
	}
}

impl ApiCall<BitcoinCrypto> for BatchTransfer {
	fn threshold_signature_payload(&self) -> <BitcoinCrypto as ChainCrypto>::Payload {
		self.bitcoin_transaction.get_signing_payloads()
	}

	fn signed(
		mut self,
		signatures: &<BitcoinCrypto as ChainCrypto>::ThresholdSignature,
		signer: <BitcoinCrypto as ChainCrypto>::AggKey,
	) -> Self {
		self.bitcoin_transaction.add_signer_and_signatures(signer, signatures.clone());
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.bitcoin_transaction.clone().finalize()
	}

	fn is_signed(&self) -> bool {
		self.bitcoin_transaction.is_signed()
	}

	fn transaction_out_id(&self) -> <BitcoinCrypto as ChainCrypto>::TransactionOutId {
		self.bitcoin_transaction.txid()
	}

	fn refresh_replay_protection(&mut self) {
		// No replay protection for Bitcoin.
	}

	fn signer(&self) -> Option<<BitcoinCrypto as ChainCrypto>::AggKey> {
		self.bitcoin_transaction
			.signer_and_signatures
			.as_ref()
			.map(|(signer, _)| *signer)
	}
}
