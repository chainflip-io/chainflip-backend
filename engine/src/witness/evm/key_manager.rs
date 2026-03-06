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

use cf_chains::evm::{
	EvmChain, EvmTransactionMetadata, SchnorrVerificationComponents, TransactionFee,
};
use ethers::{prelude::abigen, types::TransactionReceipt};
use sp_core::H256;

use crate::witness::evm::EvmTransactionClient;
use num_traits::Zero;

abigen!(KeyManager, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IKeyManager.json");

// This type is generated in the macro above.
//`Key(uint256,uint8)`
impl Key {
	/// 1 byte of pub_key_y_parity followed by 32 bytes of pub_key_x
	/// Equivalent to secp256k1::PublicKey.serialize()
	pub fn serialize(&self) -> [u8; 33] {
		let mut bytes: [u8; 33] = [0; 33];
		bytes[0] = match self.pub_key_y_parity.is_zero() {
			true => 2,
			false => 3,
		};
		bytes[1..33].copy_from_slice(&self.pub_key_x.to_big_endian());
		bytes
	}
}

use anyhow::Result;

use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_vaults::VaultKeyRotatedExternally;
use state_chain_runtime::chainflip::witnessing::pallet_hooks::{self, EvmKeyManagerEvent};

use crate::evm::event::Event;

////////////////////////////////////////////////////////
// Elections code

pub async fn handle_key_manager_events<
	T: pallet_hooks::Config<I, TargetChain: EvmChain>,
	I: 'static,
>(
	client: &impl EvmTransactionClient,
	events: Vec<Event<KeyManagerEvents>>,
	block_height: u64,
) -> Result<Vec<EvmKeyManagerEvent<T, I>>> {
	Ok(futures::future::try_join_all(events.into_iter().map(|event| {
		handle_key_manager_event::<T, I>(
			client,
			event.event_parameters,
			event.tx_hash,
			block_height,
		)
	}))
	.await?
	.into_iter()
	.flatten()
	.collect())
}

async fn handle_key_manager_event<T: pallet_hooks::Config<I, TargetChain: EvmChain>, I: 'static>(
	client: &impl EvmTransactionClient,
	event: KeyManagerEvents,
	tx_hash: H256,
	block_height: u64,
) -> Result<Option<EvmKeyManagerEvent<T, I>>> {
	Ok(Some(match event {
		KeyManagerEvents::AggKeySetByGovKeyFilter(AggKeySetByGovKeyFilter {
			new_agg_key, ..
		}) => EvmKeyManagerEvent::AggKeySetByGovKey(VaultKeyRotatedExternally {
			new_public_key: cf_chains::evm::AggKey::from_pubkey_compressed(new_agg_key.serialize()),
			block_number: block_height,
			tx_id: tx_hash,
		}),
		KeyManagerEvents::SignatureAcceptedFilter(SignatureAcceptedFilter { sig_data, .. }) => {
			let TransactionReceipt { gas_used, effective_gas_price, from, to, .. } =
				client.transaction_receipt(tx_hash).await?;

			let gas_used = gas_used
				.ok_or_else(|| {
					anyhow::anyhow!("No gas_used on Transaction receipt for tx_hash: {}", tx_hash)
				})?
				.try_into()
				.map_err(anyhow::Error::msg)?;
			let effective_gas_price = effective_gas_price
				.ok_or_else(|| {
					anyhow::anyhow!(
						"No effective_gas_price on Transaction receipt for tx_hash: {}",
						tx_hash
					)
				})?
				.try_into()
				.map_err(anyhow::Error::msg)?;

			let transaction = client.get_transaction(tx_hash).await?;
			let tx_metadata = EvmTransactionMetadata {
				contract: to.expect("To have a contract"),
				max_fee_per_gas: transaction.max_fee_per_gas,
				max_priority_fee_per_gas: transaction.max_priority_fee_per_gas,
				gas_limit: Some(transaction.gas),
			};

			EvmKeyManagerEvent::SignatureAccepted(TransactionConfirmation {
				tx_out_id: SchnorrVerificationComponents {
					s: sig_data.sig.to_big_endian(),
					k_times_g_address: sig_data.k_times_g_address.into(),
				},
				signer_id: from,
				tx_fee: TransactionFee { effective_gas_price, gas_used },
				tx_metadata,
				transaction_ref: transaction.hash,
			})
		},
		KeyManagerEvents::GovernanceActionFilter(GovernanceActionFilter { message: call_hash }) =>
			EvmKeyManagerEvent::SetWhitelistedCallHash(call_hash),
		_ => return Ok(None),
	}))
}
