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
use serde::{Deserialize, Serialize};
use sp_std::vec::Vec;

use crate::sol::SolAddress;

pub use sol_prim::*;

/// Provides alternative version of internal types that uses `Address` instead of Pubkey:
///
/// |----------------------|
/// |Type    |   Serialized|
/// |----------------------|
/// |Pubkey  |   Byte Array|
/// |Address |   bs58      |
/// |----------------------|
///
/// When serialized, these types returns Solana addresses in human readable bs58 format.
/// These are intended to be used for returning data via RPC calls only.
pub mod rpc_types {
	use super::*;

	#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TypeInfo)]
	pub struct SolInstructionRpc {
		pub program_id: SolAddress,
		pub accounts: Vec<SolAccountMetaRpc>,
		#[serde(with = "sp_core::bytes")]
		pub data: Vec<u8>,
	}

	impl From<Instruction> for SolInstructionRpc {
		fn from(value: Instruction) -> Self {
			SolInstructionRpc {
				program_id: value.program_id.into(),
				accounts: value.accounts.into_iter().map(|a| a.into()).collect(),
				data: value.data,
			}
		}
	}

	#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TypeInfo)]
	pub struct SolAccountMetaRpc {
		pub address: SolAddress,
		pub is_signer: bool,
		pub is_writable: bool,
	}

	impl From<AccountMeta> for SolAccountMetaRpc {
		fn from(value: AccountMeta) -> Self {
			SolAccountMetaRpc {
				address: value.pubkey.into(),
				is_signer: value.is_signer,
				is_writable: value.is_writable,
			}
		}
	}
}

#[derive(
	Encode,
	Decode,
	TypeInfo,
	Serialize,
	Deserialize,
	Debug,
	Copy,
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
)]
pub struct CcmAddress {
	pub pubkey: Pubkey,
	pub is_writable: bool,
}

impl From<CcmAddress> for AccountMeta {
	fn from(from: CcmAddress) -> Self {
		match from.is_writable {
			true => AccountMeta::new(from.pubkey, false),
			false => AccountMeta::new_readonly(from.pubkey, false),
		}
	}
}

#[derive(
	Encode, Decode, TypeInfo, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct CcmAccounts {
	pub cf_receiver: CcmAddress,
	pub additional_accounts: Vec<CcmAddress>,
	pub fallback_address: Pubkey,
}

impl CcmAccounts {
	pub fn additional_account_metas(self) -> Vec<AccountMeta> {
		self.additional_accounts.into_iter().map(|acc| acc.into()).collect::<Vec<_>>()
	}
}

#[test]
fn ccm_extra_accounts_encoding() {
	let extra_accounts = CcmAccounts {
		cf_receiver: CcmAddress { pubkey: Pubkey([0x11; 32]), is_writable: false },
		additional_accounts: vec![
			CcmAddress { pubkey: Pubkey([0x22; 32]), is_writable: true },
			CcmAddress { pubkey: Pubkey([0x33; 32]), is_writable: true },
		],
		fallback_address: Pubkey([0xf0; 32]),
	};

	let encoded = Encode::encode(&extra_accounts);

	// Scale encoding format:
	// cf_receiver(32 bytes, bool),
	// size_of_vec(compact encoding), additional_accounts_0(32 bytes, bool), additional_accounts_1,
	// etc..
	assert_eq!(
		encoded,
		hex_literal::hex!(
			"1111111111111111111111111111111111111111111111111111111111111111 00
			08 
			2222222222222222222222222222222222222222222222222222222222222222 01
			3333333333333333333333333333333333333333333333333333333333333333 01
			F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0"
		)
	);
}

/// Values used for testing purposes
#[cfg(any(test, feature = "runtime-integration-tests"))]
pub mod sol_test_values {
	use crate::{
		ccm_checker::{DecodedCcmAdditionalData, VersionedSolanaCcmAdditionalData},
		sol::{
			api::{DurableNonceAndAccount, VaultSwapAccountAndSender},
			signing_key::SolSigningKey,
			sol_tx_core::signer::{Signer, TestSigners},
			SolAddress, SolAddressLookupTableAccount, SolAmount, SolApiEnvironment, SolAsset,
			SolCcmAccounts, SolCcmAddress, SolComputeLimit, SolHash, SolVersionedTransaction,
		},
		CcmChannelMetadataChecked, CcmChannelMetadataUnchecked, CcmDepositMetadataChecked,
		ForeignChain, ForeignChainAddress,
	};
	use codec::Encode;
	use itertools::Itertools;
	use sol_prim::consts::{const_address, const_hash, MAX_TRANSACTION_LENGTH};
	use sp_std::vec;

	pub const VAULT_PROGRAM: SolAddress =
		const_address("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf");
	pub const VAULT_PROGRAM_DATA_ADDRESS: SolAddress =
		const_address("3oEKmL4nsw6RDZWhkYTdCUmjxDrzVkm1cWayPsvn3p57");
	pub const VAULT_PROGRAM_DATA_ACCOUNT: SolAddress =
		const_address("BttvFNSRKrkHugwDP6SpnBejCKKskHowJif1HGgBtTfG");
	// MIN_PUB_KEY per supported spl-token
	pub const USDC_TOKEN_MINT_PUB_KEY: SolAddress =
		const_address("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p");
	pub const TOKEN_VAULT_PDA_ACCOUNT: SolAddress =
		const_address("CWxWcNZR1d5MpkvmL3HgvgohztoKyCDumuZvdPyJHK3d");
	// This can be derived from the TOKEN_VAULT_PDA_ACCOUNT and the mintPubKey but we can have it
	// stored There will be a different one per each supported spl-token
	pub const USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT: SolAddress =
		const_address("GgqCE4bTwMy4QWVaTRTKJqETAgim49zNrH1dL6zXaTpd");
	pub const SWAP_ENDPOINT_DATA_ACCOUNT_ADDRESS: SolAddress =
		const_address("GgqCE4bTwMy4QWVaTRTKJqETAgim49zNrH1dL6zXaTpd");
	pub const NONCE_ACCOUNTS: [SolAddress; 10] = [
		const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
		const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
		const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
		const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
		const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
		const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
		const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
		const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
		const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
		const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
	];
	pub const SWAP_ENDPOINT_PROGRAM: SolAddress =
		const_address("35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT");
	pub const SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT: SolAddress =
		const_address("2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9");
	pub const ALT_MANAGER_PROGRAM: SolAddress =
		const_address("49XegQyykAXwzigc6u7gXbaLjhKfNadWMZwFiovzjwUw");
	pub const ADDRESS_LOOKUP_TABLE_ACCOUNT: SolAddress =
		const_address("7drVSq2ymJLNnXyCciHbNqHyzuSt1SL4iQSEThiESN2c");
	pub const EVENT_AND_SENDER_ACCOUNTS: [VaultSwapAccountAndSender; 11] = [
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("2cHcSNtikMpjxJfwwoYL3udpy7hedRExyhakk2eZ6cYA"),
			swap_sender: const_address("7tVhSXxGfZyHQem8MdZVB6SoRsrvV4H8h1rX6hwBuvEA"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("6uuU1NFyThN3KJpU9mYXkGSmd8Qgncmd9aYAWYN71VkC"),
			swap_sender: const_address("P3GYr1Z67jdBVimzFjMXQpeuew5TY5txoZ9CvqASpaP"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("DmAom3kp2ZKk9cnbWEsnbkLHkp3sx9ef1EX6GWj1JRUB"),
			swap_sender: const_address("CS7yX5TKX36ugF4bycmVQ5vqB2ZbNVC5tvtrtLP92GDW"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("CJSdHgxwHLEbTsxKsJk9UyJxUEgku2UC9GXRTzR2ieSh"),
			swap_sender: const_address("2taCR53epDtdrFZBxzKcbmv3cb5Umc5x9k2YCjmTDAnH"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("7DGwjsQEFA7XzZS9z5YbMhYGzWJSh5T78hRrU47RDTd2"),
			swap_sender: const_address("FDPzoZj951Hq92jhoFdyzAVyUjyXhL8VEnqBhyjsDhow"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("A6yYXUmZHa32mcFRnwwq8ZQKCEYUn9ewF1vWn2wsXN5a"),
			swap_sender: const_address("9bNNNU9B52VPVGm6zRccwPEexDHD1ntndD2aNu2un3ca"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("2F3365PULNzt7moa9GgHARy7Lumj5ptDQF7wDt6xeuHK"),
			swap_sender: const_address("4m5t38fJsvULKaPyWZKWjzfbvnzBGL86BTRNk5vLLUrh"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("8sCBWv9tzdf2iC4GNj61UBN6TZpzsLP5Ppv9x1ENX4HT"),
			swap_sender: const_address("A3P5kfRU1vgZn7GjNMomS8ye6GHsoHC4JoVNUotMbDPE"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("3b1FkNvnvKJ4TzKeft7wA47VfYpjkoHPE4ER13ZTNecX"),
			swap_sender: const_address("ERwuPnX66dCZqj85kH9QQJmwcVrzcczBnu8onJY2R7tG"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("Bnrp9X562krXVfaY8FnwJa3Mxp1gbDCrvGNW1qc99rKe"),
			swap_sender: const_address("2aoZg41FFnTBnuHpkfHdFsCuPz8DhN4dsUW5386XwE8g"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("EuLceVgXMaJNPT7C88pnL7DRWcf1poy9BCeWY1GL8Agd"),
			swap_sender: const_address("G1iXMtwUU76JGau9cJm6N8wBTmcsvyXuJcC7PtfU1TXZ"),
		},
	];
	pub const RAW_KEYPAIR: [u8; 32] = [
		6, 151, 150, 20, 145, 210, 176, 113, 98, 200, 192, 80, 73, 63, 133, 232, 208, 124, 81, 213,
		117, 199, 196, 243, 219, 33, 79, 217, 157, 69, 205, 140,
	];
	pub const TRANSFER_AMOUNT: SolAmount = 1_000_000_000u64;
	pub const COMPUTE_UNIT_PRICE: SolAmount = 1_000_000u64;
	pub const COMPUTE_UNIT_LIMIT: SolComputeLimit = 300_000u32;
	pub const TEST_DURABLE_NONCE: SolHash =
		const_hash("E6E2bNxGcgFyqeVRT3FSjw7YFbbMAZVQC21ZLVwrztRm");
	pub const FETCH_FROM_ACCOUNT: SolAddress =
		const_address("4Spd3kst7XsA9pdp5ArfdXxEK4xfW88eRKbyQBmMvwQj");
	pub const TRANSFER_TO_ACCOUNT: SolAddress =
		const_address("4MqL4qy2W1yXzuF3PiuSMehMbJzMuZEcBwVvrgtuhx7V");
	pub const NEW_AGG_KEY: SolAddress =
		const_address("7x7wY9yfXjRmusDEfPPCreU4bP49kmH4mqjYUXNAXJoM");

	pub const NEXT_NONCE: SolAddress = NONCE_ACCOUNTS[0];
	pub const SOL: SolAsset = SolAsset::Sol;
	pub const USDC: SolAsset = SolAsset::SolUsdc;

	// Arbitrary number used for testing
	pub const TEST_COMPUTE_LIMIT: SolComputeLimit = 300_000u32;

	pub fn durable_nonce() -> DurableNonceAndAccount {
		(NONCE_ACCOUNTS[0], TEST_DURABLE_NONCE)
	}

	pub fn api_env() -> SolApiEnvironment {
		SolApiEnvironment {
			vault_program: VAULT_PROGRAM,
			vault_program_data_account: VAULT_PROGRAM_DATA_ACCOUNT,
			token_vault_pda_account: TOKEN_VAULT_PDA_ACCOUNT,
			usdc_token_mint_pubkey: USDC_TOKEN_MINT_PUB_KEY,
			usdc_token_vault_ata: USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			swap_endpoint_program: SWAP_ENDPOINT_PROGRAM,
			swap_endpoint_program_data_account: SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT,
			alt_manager_program: ALT_MANAGER_PROGRAM,
			address_lookup_table_account: user_alt(),
		}
	}

	pub fn compute_price() -> SolAmount {
		COMPUTE_UNIT_PRICE
	}

	pub fn nonce_accounts() -> Vec<SolAddress> {
		NONCE_ACCOUNTS.to_vec()
	}

	pub fn ccm_accounts() -> SolCcmAccounts {
		SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: const_address("8pBPaVfTAcjLeNfC187Fkvi9b1XEFhRNJ95BQXXVksmH").into(),
				is_writable: true,
			},
			additional_accounts: vec![SolCcmAddress {
				pubkey: const_address("CFp37nEY6E9byYHiuxQZg6vMCnzwNrgiF9nFGT6Zwcnx").into(),
				is_writable: false,
			}],
			fallback_address: const_address("AkYRjwVHBCcE1HsjZaTFr5SrTNHPRX7PtwZxdSDMcTvb").into(),
		}
	}

	pub fn ccm_parameter_v0() -> CcmDepositMetadataChecked<ForeignChainAddress> {
		CcmDepositMetadataChecked {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
			channel_metadata: CcmChannelMetadataChecked {
				message: vec![124u8, 29u8, 15u8, 7u8].try_into().unwrap(),
				gas_budget: 0u128,
				ccm_additional_data: DecodedCcmAdditionalData::Solana(
					VersionedSolanaCcmAdditionalData::V0(ccm_accounts()),
				),
			},
		}
	}

	pub fn ccm_parameter_v1() -> CcmDepositMetadataChecked<ForeignChainAddress> {
		let mut ccm = ccm_parameter_v0();
		ccm.channel_metadata.ccm_additional_data =
			DecodedCcmAdditionalData::Solana(VersionedSolanaCcmAdditionalData::V1 {
				ccm_accounts: ccm_accounts(),
				alts: vec![user_alt().key.into()],
			});
		ccm
	}

	pub fn ccm_metadata_v0_unchecked() -> CcmChannelMetadataUnchecked {
		let ccm_metadata = ccm_parameter_v0().channel_metadata;
		CcmChannelMetadataUnchecked {
			message: ccm_metadata.message.clone(),
			gas_budget: ccm_metadata.gas_budget,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(ccm_accounts())
				.encode()
				.try_into()
				.unwrap(),
		}
	}

	pub fn ccm_metadata_v1_unchecked() -> CcmChannelMetadataUnchecked {
		let ccm_metadata = ccm_parameter_v0().channel_metadata;
		CcmChannelMetadataUnchecked {
			message: ccm_metadata.message.clone(),
			gas_budget: ccm_metadata.gas_budget,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V1 {
				ccm_accounts: ccm_accounts(),
				alts: vec![user_alt().key.into()],
			}
			.encode()
			.try_into()
			.unwrap(),
		}
	}

	pub fn agg_key() -> SolAddress {
		SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap().pubkey().into()
	}

	pub fn chainflip_alt() -> SolAddressLookupTableAccount {
		let token_vault_ata =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				TOKEN_VAULT_PDA_ACCOUNT,
				USDC_TOKEN_MINT_PUB_KEY,
			)
			.unwrap()
			.address;

		SolAddressLookupTableAccount {
			key: const_address("4EQ4ZTskvNwkBaQjBJW5grcmV5Js82sUooNLHNTpdHdi").into(),
			addresses: vec![
				vec![
					VAULT_PROGRAM,
					VAULT_PROGRAM_DATA_ADDRESS,
					VAULT_PROGRAM_DATA_ACCOUNT,
					USDC_TOKEN_MINT_PUB_KEY,
					TOKEN_VAULT_PDA_ACCOUNT,
					USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
					SWAP_ENDPOINT_DATA_ACCOUNT_ADDRESS,
					SWAP_ENDPOINT_PROGRAM,
					SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT,
					sol_prim::consts::TOKEN_PROGRAM_ID,
					sol_prim::consts::SYS_VAR_INSTRUCTIONS,
					sol_prim::consts::ASSOCIATED_TOKEN_PROGRAM_ID,
					sol_prim::consts::SYSTEM_PROGRAM_ID,
					sol_prim::consts::SYS_VAR_RECENT_BLOCKHASHES,
					token_vault_ata,
				],
				NONCE_ACCOUNTS.to_vec(),
			]
			.into_iter()
			.concat()
			.into_iter()
			.map(|a| a.into())
			.collect::<Vec<_>>(),
		}
	}

	pub fn user_alt() -> SolAddressLookupTableAccount {
		SolAddressLookupTableAccount { key: ADDRESS_LOOKUP_TABLE_ACCOUNT.into(), addresses: vec![] }
	}

	#[track_caller]
	pub fn sign_and_serialize(mut transaction: SolVersionedTransaction) -> Vec<u8> {
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let durable_nonce = durable_nonce().1.into();

		// Sign the transaction with the given signers and blockhash.
		transaction.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

		transaction
			.clone()
			.finalize_and_serialize()
			.expect("Transaction serialization must succeed")
	}

	#[track_caller]
	pub fn test_constructed_transaction_with_signer<S: Signer>(
		mut transaction: SolVersionedTransaction,
		expected_serialized_tx: Vec<u8>,
		signers: TestSigners<S>,
		blockhash: super::Hash,
	) {
		// Sign the transaction with the given singers and blockhash.
		transaction.test_only_sign(signers, blockhash);

		let serialized_tx = transaction
			.clone()
			.finalize_and_serialize()
			.expect("Transaction serialization must succeed");

		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH);

		if serialized_tx != expected_serialized_tx {
			panic!(
				"Transaction mismatch. \nTx: {:?} \nSerialized: {:?}",
				transaction,
				hex::encode(serialized_tx.clone())
			);
		}
	}

	#[track_caller]
	pub fn test_constructed_transaction(
		transaction: SolVersionedTransaction,
		expected_serialized_tx: Vec<u8>,
	) {
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let durable_nonce = durable_nonce().1.into();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![agg_key_keypair].into(),
			durable_nonce,
		);
	}
}
