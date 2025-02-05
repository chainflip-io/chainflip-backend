use crate::{
	address::EncodedAddress,
	sol::{SolAsset, SolCcmAccounts, SolPubkey, MAX_CCM_BYTES_SOL, MAX_CCM_BYTES_USDC},
	CcmChannelMetadata,
};
use cf_primitives::{Asset, ForeignChain};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sol_prim::consts::{
	ACCOUNT_KEY_LENGTH_IN_TRANSACTION, ACCOUNT_REFERENCE_LENGTH_IN_TRANSACTION, SYSTEM_PROGRAM_ID,
	SYS_VAR_INSTRUCTIONS, TOKEN_PROGRAM_ID,
};
use sp_runtime::DispatchError;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum CcmValidityError {
	CannotDecodeCcmAdditionalData,
	CcmIsTooLong,
	CcmAdditionalDataContainsInvalidAccounts,
	RedundantDataSupplied,
	InvalidDestinationAddress,
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
}

#[derive(Clone, Debug, Decode, PartialEq, Eq)]
pub enum DecodedCcmAdditionalData {
	NotRequired,
	Solana(VersionedSolanaCcmAdditionalData),
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub enum VersionedSolanaCcmAdditionalData {
	V0(SolCcmAccounts),
}

pub struct CcmValidityChecker;

impl CcmValidityCheck for CcmValidityChecker {
	/// Checks to see if a given CCM is valid. Currently this only applies to Solana chain.
	/// For Solana Chain: Performs decoding of the `cf_parameter`, and checks the expected length.
	/// Returns the decoded `cf_parameter`.
	fn check_and_decode(
		ccm: &CcmChannelMetadata,
		egress_asset: Asset,
		destination: EncodedAddress,
	) -> Result<DecodedCcmAdditionalData, CcmValidityError> {
		if ForeignChain::from(egress_asset) == ForeignChain::Solana {
			let destination_address = SolPubkey::try_from(destination)
				.map_err(|_| CcmValidityError::InvalidDestinationAddress)?;

			let asset: SolAsset = egress_asset
				.try_into()
				.expect("Only Solana chain's asset will be checked. This conversion must succeed.");

			// Check if the cf_parameter can be decoded
			match VersionedSolanaCcmAdditionalData::decode(
				&mut &ccm.ccm_additional_data.clone()[..],
			)
			.map_err(|_| CcmValidityError::CannotDecodeCcmAdditionalData)?
			{
				VersionedSolanaCcmAdditionalData::V0(ccm_accounts) => {
					// It's hard at this stage to compute exactly the length of the finally build
					// transaction from the message and the additional accounts. Duplicated
					// accounts only take one reference byte while new accounts take 32 bytes.
					// Technically it shouldn't be necessary to pass duplicated accounts as
					// it will all be executed in the same instruction. However when integrating
					// with other protocols, many of the account's values are part of a returned
					// payload from an API and it makes it cumbersome to then dedpulicate on the
					// fly and then make it match with the receiver contract. It can be done
					// but it then requires extra configuration bytes in the payload, which
					// then defeats the purpose.
					// Therefore we want to allow for duplicated accounts, both duplicated
					// within the additional accounts and with our accounts. Then we can
					// calculate the length accordingly.
					// The Chainflip accounts are anyway irrelevant to the user except for a
					// few that are acounted for here. The only relevant is the token
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
							accounts_length += ACCOUNT_KEY_LENGTH_IN_TRANSACTION;
						}
					}

					let ccm_length = ccm.message.len() + accounts_length;

					if ccm_length >
						match asset {
							SolAsset::Sol => MAX_CCM_BYTES_SOL,
							SolAsset::SolUsdc => MAX_CCM_BYTES_USDC,
						} {
						return Err(CcmValidityError::CcmIsTooLong)
					}

					Ok(DecodedCcmAdditionalData::Solana(VersionedSolanaCcmAdditionalData::V0(
						ccm_accounts,
					)))
				},
			}
		} else if !ccm.ccm_additional_data.is_empty() {
			Err(CcmValidityError::RedundantDataSupplied)
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
	use crate::sol::{sol_tx_core::sol_test_values, SolCcmAddress, SolPubkey, MAX_CCM_BYTES_SOL};

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
			message: vec![0x01; MAX_CCM_BYTES_SOL].try_into().unwrap(),
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
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_SOL + 1].try_into().unwrap();
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
			message: vec![0x01; MAX_CCM_BYTES_USDC].try_into().unwrap(),
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
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_USDC + 1].try_into().unwrap();
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
		ccm.message = [0x00; MAX_CCM_BYTES_SOL + 1].to_vec().try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol, DEST_ADDR),
			CcmValidityError::CcmIsTooLong
		);
		ccm.message = [0x00; MAX_CCM_BYTES_USDC + 1].to_vec().try_into().unwrap();
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
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
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
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
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
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
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
			message: vec![0x01; MAX_CCM_BYTES_SOL - 36].try_into().unwrap(),
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
			message: vec![0x01; MAX_CCM_BYTES_SOL - 36].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: CF_RECEIVER_ADDR, is_writable: true },
				fallback_address: FALLBACK_ADDR,
				additional_accounts: vec![
					SolCcmAddress { pubkey: SYSTEM_PROGRAM_ID.into(), is_writable: false },
					SolCcmAddress { pubkey: MOCK_ADDR, is_writable: true },
					SolCcmAddress { pubkey:  SolPubkey::try_from(DEST_ADDR).unwrap(), is_writable: true },
					SolCcmAddress { pubkey: SolPubkey::try_from(DEST_ADDR).unwrap(), is_writable: true },
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
			message: vec![0x01; MAX_CCM_BYTES_SOL - 68].try_into().unwrap(),
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
			message: vec![0x01; MAX_CCM_BYTES_USDC - 37].try_into().unwrap(),
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
			message: vec![0x01; MAX_CCM_BYTES_USDC - 36].try_into().unwrap(),
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
}
