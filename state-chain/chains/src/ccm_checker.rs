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

use crate::{
	address::EncodedAddress,
	hub::AssethubRuntimeCall,
	sol::{
		sol_tx_core::consts::{
			ACCOUNT_KEY_LENGTH_IN_TRANSACTION, ACCOUNT_REFERENCE_LENGTH_IN_TRANSACTION,
			SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS, TOKEN_PROGRAM_ID,
		},
		SolAddress, SolAsset, SolCcmAccounts, SolPubkey, MAX_CCM_USER_ALTS, MAX_USER_CCM_BYTES_SOL,
		MAX_USER_CCM_BYTES_USDC,
	},
	CcmAdditionalData, CcmChannelMetadata, Chain, ForeignChainAddress,
};
use cf_primitives::{Asset, ForeignChain};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;
use sp_std::{collections::btree_set::BTreeSet, vec, vec::Vec};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum CcmValidityError {
	CannotDecodeCcmAdditionalData,
	CcmIsTooLong,
	CcmAdditionalDataContainsInvalidAccounts,
	RedundantDataSupplied,
	InvalidDestinationAddress,
	TooManyAddressLookupTables,
}
impl From<CcmValidityError> for DispatchError {
	fn from(value: CcmValidityError) -> Self {
		match value {
			CcmValidityError::CannotDecodeCcmAdditionalData =>
				"Invalid Ccm: Cannot decode additional data".into(),
			CcmValidityError::CcmIsTooLong => "Invalid Ccm: message too long".into(),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts =>
				"Invalid Ccm: additional data contains invalid accounts".into(),
			CcmValidityError::RedundantDataSupplied =>
				"Invalid Ccm: Additional data supplied but they will not be used".into(),
			CcmValidityError::InvalidDestinationAddress =>
				"Invalid Ccm: Destination address is not compatible with the target Chain.".into(),
			CcmValidityError::TooManyAddressLookupTables =>
				"Invalid Ccm: Too many Address Lookup tables supplied".into(),
		}
	}
}

pub trait CcmValidityCheck {
	fn check_and_decode(
		_ccm: &CcmChannelMetadata,
		_egress_asset: cf_primitives::Asset,
		_destination: EncodedAddress,
	) -> Result<DecodedCcmAdditionalData, CcmValidityError> {
		Ok(DecodedCcmAdditionalData::NotRequired)
	}

	fn decode_unchecked(
		_ccm: CcmAdditionalData,
		_chain: ForeignChain,
	) -> Result<DecodedCcmAdditionalData, CcmValidityError> {
		Ok(DecodedCcmAdditionalData::NotRequired)
	}
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub enum DecodedCcmAdditionalData {
	NotRequired,
	Solana(VersionedSolanaCcmAdditionalData),
}

impl DecodedCcmAdditionalData {
	/// Attempt to extract the fallback address from the decoded ccm additional data.
	/// Will only return Some(addr) if fallback address exists and matches the target `Chain`.
	pub fn refund_address<C: Chain>(&self) -> Option<C::ChainAccount> {
		match self {
			DecodedCcmAdditionalData::Solana(additional_data) => ForeignChainAddress::from(
				SolAddress::from(additional_data.ccm_accounts().fallback_address),
			)
			.try_into()
			.ok(),
			_ => None,
		}
	}
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub enum VersionedSolanaCcmAdditionalData {
	V0(SolCcmAccounts),
	V1 { ccm_accounts: SolCcmAccounts, alts: Vec<SolAddress> },
}

impl VersionedSolanaCcmAdditionalData {
	pub fn ccm_accounts(&self) -> SolCcmAccounts {
		match self {
			VersionedSolanaCcmAdditionalData::V0(ccm_accounts) => ccm_accounts.clone(),
			VersionedSolanaCcmAdditionalData::V1 { ccm_accounts, .. } => ccm_accounts.clone(),
		}
	}

	pub fn address_lookup_tables(&self) -> Vec<SolAddress> {
		match self {
			VersionedSolanaCcmAdditionalData::V0(..) => vec![],
			VersionedSolanaCcmAdditionalData::V1 { alts, .. } => alts.clone(),
		}
	}
}

pub struct CcmValidityChecker;

impl CcmValidityCheck for CcmValidityChecker {
	/// Checks to see if a given CCM is valid. Currently this only applies to Solana and Assethub
	/// chains. For Solana Chain: Performs decoding of the `cf_parameter`, and checks the expected
	/// length. For Assethub Chain: Decodes the message into a supported extrinsic of the
	/// PolkadotXcm pallet. Returns the decoded `cf_parameter`.
	fn check_and_decode(
		ccm: &CcmChannelMetadata,
		egress_asset: Asset,
		destination: EncodedAddress,
	) -> Result<DecodedCcmAdditionalData, CcmValidityError> {
		match ForeignChain::from(egress_asset) {
			ForeignChain::Solana => {
				let destination_address = SolPubkey::try_from(destination)
					.map_err(|_| CcmValidityError::InvalidDestinationAddress)?;

				let asset: SolAsset = egress_asset.try_into().expect(
					"Only Solana chain's asset will be checked. This conversion must succeed.",
				);

				// Check if the cf_parameter can be decoded
				let decoded_data = VersionedSolanaCcmAdditionalData::decode(
					&mut &ccm.ccm_additional_data.clone()[..],
				)
				.map_err(|_| CcmValidityError::CannotDecodeCcmAdditionalData)?;

				let ccm_accounts = decoded_data.ccm_accounts();
				let address_lookup_tables = decoded_data.address_lookup_tables();
				let num_address_lookup_tables = address_lookup_tables.len();

				if num_address_lookup_tables > MAX_CCM_USER_ALTS as usize {
					return Err(CcmValidityError::TooManyAddressLookupTables)
				}

				// We calculate the final length of the CCM transaction to fail early if it's
				// certain that the egress will fail.
				// Chainflip uses Versioned transactions regardless of the user passing an ALT or
				// not. If the user doesn't pass any additional address lookup table (ALT) the
				// final length of the egress can be calculated deterministically. However, a
				// user ALT makes it so we can't exactly calculate the final length, since we
				// can't know the ALT content beforehand. Therefore, we calculate the most
				// optimistic scenario when an ALT is passed and it might fail to build on
				// egress but we rely on the user to provide valid ALTs that will make it so the
				// egress transaction will succeed. If that was not the case and the CCM egress
				// were to fail, the user would be refunded to a fallback address.
				let (lookup_tables_length, extra_buffer, bytes_per_new_account) =
					if num_address_lookup_tables > 0 {
						// Each empty lookup table is 34 bytes -> 32 bytes for address plus 2 for
						// vector lengths (write_indexes and readonly_indexes).
						let lookup_tables_length =
							num_address_lookup_tables * (ACCOUNT_KEY_LENGTH_IN_TRANSACTION + 2);

						// The most optimistic scenario is the ALT containing both the CfReceiver
						// and the destination address as well as all the ccm additional
						// accounts. That will allow for an extra amount of bytes that is
						// available for the user transaction.
						let extra_buffer = (ACCOUNT_KEY_LENGTH_IN_TRANSACTION * 2)
							.saturating_sub(2 * ACCOUNT_REFERENCE_LENGTH_IN_TRANSACTION);

						// Each non-repeated account would take an extra byte on the address table
						// lookups.
						(
							lookup_tables_length,
							extra_buffer,
							ACCOUNT_REFERENCE_LENGTH_IN_TRANSACTION,
						)
					} else {
						// Without lookup tables each new non-repeated account will take a full
						// account.
						(0, 0, ACCOUNT_KEY_LENGTH_IN_TRANSACTION)
					};

				// Technically it shouldn't be necessary to pass duplicated accounts as
				// it will all be executed in the same instruction. However, when integrating
				// with other protocols, many of the accounts are part of a returned
				// payload from an API and it makes it cumbersome to then deduplicate on the
				// fly and then make it match with the receiver contract. It can be done
				// but it then requires extra configuration bytes in the payload, which
				// then defeats the purpose of decreasing the payload length.
				// Therefore we want to account for duplicated accounts, both duplicated
				// within the additional accounts and with our accounts. Then we can
				// calculate the length accordingly.
				// The only Chainflip accounts that are relevant to the user for deduplication
				// purposes are used when initializing the `seen_addresses` set.
				let mut seen_addresses = BTreeSet::from_iter([
					SYSTEM_PROGRAM_ID,
					SYS_VAR_INSTRUCTIONS,
					destination_address.into(),
					ccm_accounts.cf_receiver.pubkey.into(),
				]);

				if asset == SolAsset::SolUsdc {
					seen_addresses.insert(TOKEN_PROGRAM_ID);
				}
				let mut accounts_length = ccm_accounts.additional_accounts.len() *
					ACCOUNT_REFERENCE_LENGTH_IN_TRANSACTION;

				for ccm_address in &ccm_accounts.additional_accounts {
					if seen_addresses.insert(ccm_address.pubkey.into()) {
						accounts_length += bytes_per_new_account;
					}
				}

				let ccm_length = (ccm.message.len() + accounts_length + lookup_tables_length)
					.saturating_sub(extra_buffer);

				if ccm_length >
					match asset {
						SolAsset::Sol => MAX_USER_CCM_BYTES_SOL,
						SolAsset::SolUsdc => MAX_USER_CCM_BYTES_USDC,
					} {
					return Err(CcmValidityError::CcmIsTooLong)
				}

				Ok(DecodedCcmAdditionalData::Solana(decoded_data))
			},
			ForeignChain::Assethub =>
				<AssethubRuntimeCall as codec::Decode>::decode(&mut ccm.message.as_ref())
					.map(|_| DecodedCcmAdditionalData::NotRequired)
					.map_err(|_| CcmValidityError::CannotDecodeCcmAdditionalData),
			_ =>
				if !ccm.ccm_additional_data.is_empty() {
					Err(CcmValidityError::RedundantDataSupplied)
				} else {
					Ok(DecodedCcmAdditionalData::NotRequired)
				},
		}
	}

	/// Decodes the `ccm_additional_data` without any additional checks.
	/// Only fail if given bytes cannot be decoded into `VersionedSolanaCcmAdditionalData`.
	fn decode_unchecked(
		ccm_additional_data: CcmAdditionalData,
		chain: ForeignChain,
	) -> Result<DecodedCcmAdditionalData, CcmValidityError> {
		if chain == ForeignChain::Solana {
			VersionedSolanaCcmAdditionalData::decode(&mut &ccm_additional_data[..])
				.map(DecodedCcmAdditionalData::Solana)
				.map_err(|_| CcmValidityError::CannotDecodeCcmAdditionalData)
		} else {
			Ok(DecodedCcmAdditionalData::NotRequired)
		}
	}
}

/// Checks if the given CCM accounts contains any blacklisted accounts.
pub fn check_ccm_for_blacklisted_accounts(
	ccm_accounts: &SolCcmAccounts,
	blacklisted_accounts: Vec<SolPubkey>,
) -> Result<(), CcmValidityError> {
	blacklisted_accounts.into_iter().try_for_each(|blacklisted_account| {
		(ccm_accounts.cf_receiver.pubkey != blacklisted_account &&
			!ccm_accounts
				.additional_accounts
				.iter()
				.any(|acc| acc.pubkey == blacklisted_account))
		.then_some(())
		.ok_or(CcmValidityError::CcmAdditionalDataContainsInvalidAccounts)
	})
}

#[cfg(test)]
mod test {
	use codec::Encode;
	use frame_support::{assert_err, assert_ok};
	use Asset;

	use super::*;
	use crate::sol::{
		sol_tx_core::sol_test_values::{self, ccm_accounts, ccm_parameter_v1, user_alt},
		SolCcmAddress, SolPubkey, MAX_USER_CCM_BYTES_SOL,
	};

	pub const DEST_ADDR: EncodedAddress = EncodedAddress::Sol([0x00; 32]);
	pub const MOCK_ADDR: SolPubkey = SolPubkey([0x01; 32]);
	pub const CF_RECEIVER_ADDR: SolPubkey = SolPubkey([0xff; 32]);
	pub const FALLBACK_ADDR: SolPubkey = SolPubkey([0xf0; 32]);
	pub const INVALID_DEST_ADDR: EncodedAddress = EncodedAddress::Eth([0x00; 20]);

	#[test]
	fn can_verify_valid_ccm() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, DEST_ADDR),
			Ok(DecodedCcmAdditionalData::Solana(VersionedSolanaCcmAdditionalData::V0(
				sol_test_values::ccm_accounts()
			)))
		);
	}

	#[test]
	fn can_check_cf_parameter_decoding() {
		let ccm = CcmChannelMetadata {
			message: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
			gas_budget: 1,
			ccm_additional_data: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
		};

		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, DEST_ADDR),
			CcmValidityError::CannotDecodeCcmAdditionalData
		);
	}

	#[test]
	fn can_check_for_ccm_length_sol() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_USER_CCM_BYTES_SOL].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				additional_accounts: vec![],
				fallback_address: FALLBACK_ADDR,
			})
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm(), Asset::Sol, DEST_ADDR));

		// Length check for Sol
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_USER_CCM_BYTES_SOL + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::Sol, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);

		let mut invalid_ccm = ccm();
		invalid_ccm.ccm_additional_data = VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
			additional_accounts: vec![SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true }],
			fallback_address: FALLBACK_ADDR,
		})
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::Sol, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn can_check_for_ccm_length_usdc() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_USER_CCM_BYTES_USDC].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				fallback_address: FALLBACK_ADDR,
				additional_accounts: vec![],
			})
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm(), Asset::SolUsdc, DEST_ADDR));

		// Length check for SolUsdc
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_USER_CCM_BYTES_USDC + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::SolUsdc, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);

		let mut invalid_ccm = ccm();
		invalid_ccm.ccm_additional_data = VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
			additional_accounts: vec![SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true }],
			fallback_address: FALLBACK_ADDR,
		})
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::SolUsdc, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn can_check_for_redundant_data() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Ok for Solana Chain
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, DEST_ADDR));

		// Fails for non-solana chains
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Btc, DEST_ADDR),
			CcmValidityError::RedundantDataSupplied,
		);
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Dot, DEST_ADDR),
			CcmValidityError::RedundantDataSupplied,
		);
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Eth, DEST_ADDR),
			CcmValidityError::RedundantDataSupplied,
		);
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::ArbEth, DEST_ADDR),
			CcmValidityError::RedundantDataSupplied,
		);
	}

	#[test]
	fn only_check_against_solana_chain() {
		let mut ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Only fails for Solana chain.
		ccm.message = [0x00; MAX_USER_CCM_BYTES_SOL + 1].to_vec().try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);
		ccm.message = [0x00; MAX_USER_CCM_BYTES_USDC + 1].to_vec().try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::SolUsdc, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);

		// Always valid on other chains.
		ccm.ccm_additional_data.clear();
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Eth, DEST_ADDR),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Btc, DEST_ADDR),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Flip, DEST_ADDR),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Usdt, DEST_ADDR),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Usdc, DEST_ADDR),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::ArbUsdc, DEST_ADDR),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::ArbEth, DEST_ADDR),
			DecodedCcmAdditionalData::NotRequired
		);
	}

	#[test]
	fn can_check_for_blacklisted_account() {
		let blacklisted_accounts = || {
			vec![sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(), sol_test_values::agg_key().into()]
		};

		// Token vault PDA is blacklisted
		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(),
				is_writable: true,
			},
			additional_accounts: vec![
				SolCcmAddress { pubkey: MOCK_ADDR, is_writable: false },
				SolCcmAddress { pubkey: SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: FALLBACK_ADDR,
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
		);

		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
			additional_accounts: vec![
				SolCcmAddress {
					pubkey: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(),
					is_writable: false,
				},
				SolCcmAddress { pubkey: SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: FALLBACK_ADDR,
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
		);

		// Agg key is blacklisted
		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: sol_test_values::agg_key().into(),
				is_writable: true,
			},
			additional_accounts: vec![
				SolCcmAddress { pubkey: MOCK_ADDR, is_writable: false },
				SolCcmAddress { pubkey: SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: FALLBACK_ADDR,
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
		);

		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
			additional_accounts: vec![
				SolCcmAddress { pubkey: sol_test_values::agg_key().into(), is_writable: false },
				SolCcmAddress { pubkey: SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: FALLBACK_ADDR,
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
		);
	}
	#[test]
	fn can_check_length_native_duplicated() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_USER_CCM_BYTES_SOL - 36].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				fallback_address: FALLBACK_ADDR,
				additional_accounts: vec![
					SolCcmAddress { pubkey: SYSTEM_PROGRAM_ID.into(), is_writable: false },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
				],
			})
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm(), Asset::Sol, DEST_ADDR));
	}
	#[test]
	fn can_check_length_duplicated_with_destination_address() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_USER_CCM_BYTES_SOL - 36].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				fallback_address: FALLBACK_ADDR,
				additional_accounts: vec![
					SolCcmAddress { pubkey: SYSTEM_PROGRAM_ID.into(), is_writable: false },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress {
						pubkey: SolPubkey::try_from(DEST_ADDR).unwrap(),
						is_writable: true,
					},
					SolCcmAddress {
						pubkey: SolPubkey::try_from(DEST_ADDR).unwrap(),
						is_writable: true,
					},
				],
			})
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm(), Asset::Sol, DEST_ADDR));
	}
	#[test]
	fn can_check_length_native_duplicated_fail() {
		let invalid_ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_USER_CCM_BYTES_SOL - 68].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				fallback_address: FALLBACK_ADDR,
				additional_accounts: vec![
					SolCcmAddress { pubkey: SYSTEM_PROGRAM_ID.into(), is_writable: false },
					SolCcmAddress { pubkey: TOKEN_PROGRAM_ID.into(), is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
				],
			})
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm(), Asset::Sol, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);
	}
	#[test]
	fn can_check_length_usdc_duplicated() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_USER_CCM_BYTES_USDC - 37].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				fallback_address: FALLBACK_ADDR,
				additional_accounts: vec![
					SolCcmAddress { pubkey: SYSTEM_PROGRAM_ID.into(), is_writable: false },
					SolCcmAddress { pubkey: TOKEN_PROGRAM_ID.into(), is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
				],
			})
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm(), Asset::SolUsdc, DEST_ADDR));
	}
	#[test]
	fn can_check_length_usdc_duplicated_fail() {
		let invalid_ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_USER_CCM_BYTES_USDC - 36].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				fallback_address: FALLBACK_ADDR,
				additional_accounts: vec![
					SolCcmAddress { pubkey: SYSTEM_PROGRAM_ID.into(), is_writable: false },
					SolCcmAddress { pubkey: TOKEN_PROGRAM_ID.into(), is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
				],
			})
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm(), Asset::SolUsdc, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn can_verify_destination_address() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, INVALID_DEST_ADDR),
			Err(CcmValidityError::InvalidDestinationAddress)
		);
	}

	#[test]
	fn can_decode_unchecked() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_ok!(CcmValidityChecker::decode_unchecked(
			ccm.ccm_additional_data.clone(),
			ForeignChain::Solana
		));
		assert_eq!(
			CcmValidityChecker::decode_unchecked(
				ccm.ccm_additional_data.clone(),
				ForeignChain::Ethereum
			),
			Ok(DecodedCcmAdditionalData::NotRequired)
		);
	}

	#[test]
	fn can_decode_unchecked_ccm_v1() {
		let ccm = sol_test_values::ccm_parameter_v1().channel_metadata;
		assert_ok!(CcmValidityChecker::decode_unchecked(
			ccm.ccm_additional_data.clone(),
			ForeignChain::Solana
		));
		assert_eq!(
			CcmValidityChecker::decode_unchecked(
				ccm.ccm_additional_data.clone(),
				ForeignChain::Ethereum
			),
			Ok(DecodedCcmAdditionalData::NotRequired)
		);
	}

	#[test]
	fn additional_data_v1_support_works() {
		let mut ccm = sol_test_values::ccm_parameter_v1().channel_metadata;
		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, DEST_ADDR),
			Ok(DecodedCcmAdditionalData::Solana(VersionedSolanaCcmAdditionalData::V1 {
				ccm_accounts: sol_test_values::ccm_accounts(),
				alts: vec![user_alt().key.into()],
			}))
		);

		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Eth, DEST_ADDR),
			Err(CcmValidityError::RedundantDataSupplied)
		);

		ccm.ccm_additional_data.clear();
		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Eth, DEST_ADDR),
			Ok(DecodedCcmAdditionalData::NotRequired)
		);

		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, INVALID_DEST_ADDR),
			Err(CcmValidityError::InvalidDestinationAddress)
		);
	}

	#[test]
	fn can_check_for_too_many_alts() {
		let mut ccm = ccm_parameter_v1().channel_metadata;
		ccm.ccm_additional_data = codec::Encode::encode(&VersionedSolanaCcmAdditionalData::V1 {
			ccm_accounts: ccm_accounts(),
			alts: (0..=MAX_CCM_USER_ALTS).map(|i| SolAddress([i; 32])).collect(),
		})
		.try_into()
		.unwrap();

		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, DEST_ADDR),
			Err(CcmValidityError::TooManyAddressLookupTables)
		);
	}
}
