use crate::{
	sol::{
		SolAsset, SolCcmAccounts, SolPubkey, CCM_BYTES_PER_ACCOUNT, MAX_CCM_BYTES_SOL,
		MAX_CCM_BYTES_USDC,
	},
	CcmChannelMetadata,
};
use cf_primitives::{Asset, ForeignChain};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum CcmValidityError {
	CannotDecodeCcmAdditionalData,
	CcmIsTooLong,
	CcmAdditionalDataContainsInvalidAccounts,
	RedundantDataSupplied,
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
		}
	}
}

pub trait CcmValidityCheck {
	fn check_and_decode(
		_ccm: &CcmChannelMetadata,
		_egress_asset: cf_primitives::Asset,
	) -> Result<DecodedCcmAdditionalData, CcmValidityError> {
		Ok(DecodedCcmAdditionalData::NotRequired)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodedCcmAdditionalData {
	Solana(SolCcmAccounts),
	NotRequired,
}

pub struct CcmValidityChecker;

impl CcmValidityCheck for CcmValidityChecker {
	/// Checks to see if a given CCM is valid. Currently this only applies to Solana chain.
	/// For Solana Chain: Performs decoding of the `cf_parameter`, and checks the expected length.
	/// Returns the decoded `cf_parameter`.
	fn check_and_decode(
		ccm: &CcmChannelMetadata,
		egress_asset: Asset,
	) -> Result<DecodedCcmAdditionalData, CcmValidityError> {
		if ForeignChain::from(egress_asset) == ForeignChain::Solana {
			// Check if the cf_parameter can be decoded
			let ccm_accounts = SolCcmAccounts::decode(&mut &ccm.ccm_additional_data.clone()[..])
				.map_err(|_| CcmValidityError::CannotDecodeCcmAdditionalData)?;
			let asset: SolAsset = egress_asset
				.try_into()
				.expect("Only Solana chain's asset will be checked. This conversion must succeed.");

			// Length of CCM = length of message + total no. remaining_accounts * constant;
			let ccm_length =
				ccm.message.len() + ccm_accounts.remaining_accounts.len() * CCM_BYTES_PER_ACCOUNT;
			if ccm_length >
				match asset {
					SolAsset::Sol => MAX_CCM_BYTES_SOL,
					SolAsset::SolUsdc => MAX_CCM_BYTES_USDC,
				} {
				return Err(CcmValidityError::CcmIsTooLong)
			}

			Ok(DecodedCcmAdditionalData::Solana(ccm_accounts))
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
				.remaining_accounts
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

	#[test]
	fn can_verify_valid_ccm() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_eq!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol),
			Ok(DecodedCcmAdditionalData::Solana(sol_test_values::ccm_accounts()))
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
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol),
			CcmValidityError::CannotDecodeCcmAdditionalData
		);
	}

	#[test]
	fn can_check_for_ccm_length_sol() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_CCM_BYTES_SOL].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
				remaining_accounts: vec![],
				fallback_address: SolPubkey([0xf0; 32]),
			}
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm(), Asset::Sol));

		// Length check for Sol
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_SOL + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::Sol),
			CcmValidityError::CcmIsTooLong
		);

		let mut invalid_ccm = ccm();
		invalid_ccm.ccm_additional_data = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
			remaining_accounts: vec![SolCcmAddress {
				pubkey: SolPubkey([0x01; 32]),
				is_writable: true,
			}],
			fallback_address: SolPubkey([0xf0; 32]),
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::Sol),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn can_check_for_ccm_length_usdc() {
		let ccm = || CcmChannelMetadata {
			message: vec![0x01; MAX_CCM_BYTES_USDC].try_into().unwrap(),
			gas_budget: 0,
			ccm_additional_data: SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
				fallback_address: SolPubkey([0xf0; 32]),
				remaining_accounts: vec![],
			}
			.encode()
			.try_into()
			.unwrap(),
		};
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm(), Asset::SolUsdc));

		// Length check for SolUsdc
		let mut invalid_ccm = ccm();
		invalid_ccm.message = vec![0x01; MAX_CCM_BYTES_USDC + 1].try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);

		let mut invalid_ccm = ccm();
		invalid_ccm.ccm_additional_data = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
			remaining_accounts: vec![SolCcmAddress {
				pubkey: SolPubkey([0x01; 32]),
				is_writable: true,
			}],
			fallback_address: SolPubkey([0xf0; 32]),
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&invalid_ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);
	}

	#[test]
	fn can_check_for_redundant_data() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Ok for Solana Chain
		assert_ok!(CcmValidityChecker::check_and_decode(&ccm, Asset::Sol));

		// Fails for non-solana chains
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Btc),
			CcmValidityError::RedundantDataSupplied,
		);
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Dot),
			CcmValidityError::RedundantDataSupplied,
		);
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Eth),
			CcmValidityError::RedundantDataSupplied,
		);
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::ArbEth),
			CcmValidityError::RedundantDataSupplied,
		);
	}

	#[test]
	fn only_check_against_solana_chain() {
		let mut ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Only fails for Solana chain.
		ccm.message = [0x00; MAX_CCM_BYTES_SOL + 1].to_vec().try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Sol),
			CcmValidityError::CcmIsTooLong
		);
		ccm.message = [0x00; MAX_CCM_BYTES_USDC + 1].to_vec().try_into().unwrap();
		assert_err!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::SolUsdc),
			CcmValidityError::CcmIsTooLong
		);

		// Always valid on other chains.
		ccm.ccm_additional_data.clear();
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Eth),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Btc),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Flip),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Usdt),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::Usdc),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::ArbUsdc),
			DecodedCcmAdditionalData::NotRequired
		);
		assert_ok!(
			CcmValidityChecker::check_and_decode(&ccm, Asset::ArbEth),
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
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: SolPubkey([0xf0; 32]),
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
		);

		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: crate::sol::SolPubkey([0x01; 32]),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress {
					pubkey: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(),
					is_writable: false,
				},
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: SolPubkey([0xf0; 32]),
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
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: SolPubkey([0xf0; 32]),
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
		);

		let ccm_accounts = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: crate::sol::SolPubkey([0x01; 32]),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: sol_test_values::agg_key().into(), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
			fallback_address: SolPubkey([0xf0; 32]),
		};
		assert_err!(
			check_ccm_for_blacklisted_accounts(&ccm_accounts, blacklisted_accounts()),
			CcmValidityError::CcmAdditionalDataContainsInvalidAccounts
		);
	}
}
