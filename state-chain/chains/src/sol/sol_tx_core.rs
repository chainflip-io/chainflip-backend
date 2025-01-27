use codec::{Decode, Encode};

use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::vec::Vec;

use crate::sol::SolAddress;

pub mod address_derivation;
pub mod bpf_loader_instructions;
pub mod compute_budget;
pub mod primitives;
pub mod program;
pub mod program_instructions;
pub mod short_vec;
#[cfg(feature = "std")]
pub mod signer;
pub mod token_instructions;
pub mod transaction;

pub use primitives::*;
pub use transaction::legacy::{Message as LegacyMessage, Transaction as LegacyTransaction};

pub mod tests;

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

/// Values used for testing purposes
#[cfg(any(test, feature = "runtime-integration-tests"))]
pub mod sol_test_values {
	use crate::{
		ccm_checker::VersionedSolanaCcmAdditionalData,
		sol::{
			api::{DurableNonceAndAccount, VaultSwapAccountAndSender},
			signing_key::SolSigningKey,
			sol_tx_core::signer::{Signer, TestSigners},
			SolAddress, SolAmount, SolApiEnvironment, SolAsset, SolCcmAccounts, SolCcmAddress,
			SolComputeLimit, SolHash,
		},
		CcmChannelMetadata, CcmDepositMetadata, ForeignChain, ForeignChainAddress,
	};
	use sol_prim::consts::{const_address, const_hash};
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

	pub fn ccm_parameter() -> CcmDepositMetadata {
		CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![124u8, 29u8, 15u8, 7u8].try_into().unwrap(), // CCM message
				gas_budget: 0u128,                                         // unused
				ccm_additional_data: codec::Encode::encode(&VersionedSolanaCcmAdditionalData::V0(
					ccm_accounts(),
				))
				.try_into()
				.expect("Test data cannot be too long"), // Extra addresses
			},
		}
	}

	pub fn agg_key() -> SolAddress {
		SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap().pubkey().into()
	}

	#[track_caller]
	pub fn test_constructed_transaction_with_signer<S: Signer>(
		mut transaction: crate::sol::SolLegacyTransaction,
		expected_serialized_tx: Vec<u8>,
		signers: TestSigners<S>,
		blockhash: super::Hash,
	) {
		// Sign the transaction with the given singers and blockhash.
		transaction.sign(signers, blockhash);

		let serialized_tx = transaction
			.clone()
			.finalize_and_serialize()
			.expect("Transaction serialization must succeed");

		assert!(serialized_tx.len() <= sol_prim::consts::MAX_TRANSACTION_LENGTH);

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
		transaction: crate::sol::SolLegacyTransaction,
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
