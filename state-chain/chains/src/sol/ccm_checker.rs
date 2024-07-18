use crate::{
	sol::{api::SolanaEnvironment, SolAsset, SolCcmAccounts},
	CcmChannelMetadata, CcmValidityChecker, CcmValidityError,
};
use codec::Decode;
use core::marker::PhantomData;
use sol_prim::consts::{
	MAX_CCM_EXTRA_ACCOUNTS, MAX_CCM_MESSAGE_LENGTH_SOL, MAX_CCM_MESSAGE_LENGTH_USDC,
};

pub struct SolanaCcmValidityChecker<Environment> {
	_phantom: PhantomData<Environment>,
}

impl<Environment: SolanaEnvironment> CcmValidityChecker for SolanaCcmValidityChecker<Environment> {
	fn is_valid(
		ccm: &CcmChannelMetadata,
		egress_asset: cf_primitives::Asset,
	) -> Result<(), CcmValidityError> {
		// Check if the cf_parameter can be decoded
		let ccm_accounts = SolCcmAccounts::decode(&mut &ccm.cf_parameters.clone()[..])
			.map_err(|_| CcmValidityError::CannotDecodeCfParameters)?;
		let asset: SolAsset =
			egress_asset.try_into().map_err(|_| CcmValidityError::UnsupportedAsset)?;

		// Ensure the length is within limit.
		if ccm.message.len() >
			match asset {
				SolAsset::Sol => MAX_CCM_MESSAGE_LENGTH_SOL,
				SolAsset::SolUsdc => MAX_CCM_MESSAGE_LENGTH_USDC,
			} {
			return Err(CcmValidityError::MessageTooLong)
		}
		if ccm_accounts.remaining_accounts.len() > MAX_CCM_EXTRA_ACCOUNTS {
			return Err(CcmValidityError::CfParametersContainsTooManyAccounts)
		}

		// Check if the parameter accounts are valid
		if let Ok(api_env) = Environment::api_environment() {
			let token_pda = api_env.token_vault_pda_account.into();
			if ccm_accounts.cf_receiver.pubkey == token_pda ||
				ccm_accounts.remaining_accounts.iter().any(|acc| acc.pubkey == token_pda)
			{
				return Err(CcmValidityError::CfParametersContainsInvalidAccounts)
			}
		};
		if let Ok(agg_key) = Environment::current_agg_key() {
			let agg_key = agg_key.into();
			if ccm_accounts.cf_receiver.pubkey == agg_key ||
				ccm_accounts.remaining_accounts.iter().any(|acc| acc.pubkey == agg_key)
			{
				return Err(CcmValidityError::CfParametersContainsInvalidAccounts)
			}
		};

		Ok(())
	}
}

#[cfg(test)]
mod test {
	use codec::Encode;
	use frame_support::{assert_err, assert_ok};

	use super::*;
	use crate::{
		sol::{
			api::{
				AllNonceAccounts, ApiEnvironment, ComputePrice, CurrentAggKey, DurableNonce,
				DurableNonceAndAccount,
			},
			signing_key::SolSigningKey,
			sol_tx_core::{signer::Signer, sol_test_values},
			SolAddress, SolAmount, SolApiEnvironment, SolCcmAddress,
		},
		ChainEnvironment,
	};

	pub struct MockEnv;

	impl ChainEnvironment<ApiEnvironment, SolApiEnvironment> for MockEnv {
		fn lookup(_s: ApiEnvironment) -> Option<SolApiEnvironment> {
			Some(SolApiEnvironment {
				vault_program: sol_test_values::VAULT_PROGRAM,
				vault_program_data_account: sol_test_values::VAULT_PROGRAM_DATA_ACCOUNT,
				token_vault_pda_account: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT,
				usdc_token_mint_pubkey: sol_test_values::USDC_TOKEN_MINT_PUB_KEY,
				usdc_token_vault_ata: sol_test_values::USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			})
		}
	}

	impl ChainEnvironment<CurrentAggKey, SolAddress> for MockEnv {
		fn lookup(_s: CurrentAggKey) -> Option<SolAddress> {
			Some(
				SolSigningKey::from_bytes(&sol_test_values::RAW_KEYPAIR)
					.expect("Key pair generation must succeed")
					.pubkey()
					.into(),
			)
		}
	}

	impl ChainEnvironment<ComputePrice, SolAmount> for MockEnv {
		fn lookup(_s: ComputePrice) -> Option<u64> {
			None
		}
	}

	impl ChainEnvironment<DurableNonce, DurableNonceAndAccount> for MockEnv {
		fn lookup(_s: DurableNonce) -> Option<DurableNonceAndAccount> {
			None
		}
	}

	impl ChainEnvironment<AllNonceAccounts, Vec<DurableNonceAndAccount>> for MockEnv {
		fn lookup(_s: AllNonceAccounts) -> Option<Vec<DurableNonceAndAccount>> {
			None
		}
	}

	impl SolanaEnvironment for MockEnv {}

	#[test]
	fn can_verify_valid_ccm() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_ok!(SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Sol));
	}

	#[test]
	fn can_check_cf_parameter_decoding() {
		let ccm = CcmChannelMetadata {
			message: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
			gas_budget: 1,
			cf_parameters: vec![0x01, 0x02, 0x03, 0x04, 0x05].try_into().unwrap(),
		};

		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Sol),
			CcmValidityError::CannotDecodeCfParameters
		);
	}

	#[test]
	fn can_check_ccm_length() {
		let mut ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Can check message for Sol egress
		ccm.message = [0x00; MAX_CCM_MESSAGE_LENGTH_SOL].to_vec().try_into().unwrap();
		assert_ok!(SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Sol));
		ccm.message = [0x00; MAX_CCM_MESSAGE_LENGTH_SOL + 1].to_vec().try_into().unwrap();
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Sol),
			CcmValidityError::MessageTooLong
		);

		// Can check message for SolUsdc egress
		ccm.message = [0x00; MAX_CCM_MESSAGE_LENGTH_USDC].to_vec().try_into().unwrap();
		assert_ok!(SolanaCcmValidityChecker::<MockEnv>::is_valid(
			&ccm,
			cf_primitives::Asset::SolUsdc
		));
		ccm.message = [0x00; MAX_CCM_MESSAGE_LENGTH_USDC + 1].to_vec().try_into().unwrap();
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::SolUsdc),
			CcmValidityError::MessageTooLong
		);
	}

	#[test]
	fn can_check_supported_asset() {
		let ccm = sol_test_values::ccm_parameter().channel_metadata;
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Eth),
			CcmValidityError::UnsupportedAsset
		);
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::ArbUsdc),
			CcmValidityError::UnsupportedAsset
		);
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Flip),
			CcmValidityError::UnsupportedAsset
		);
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Usdc),
			CcmValidityError::UnsupportedAsset
		);
	}

	#[test]
	fn can_for_blacklisted_account() {
		let mut ccm = sol_test_values::ccm_parameter().channel_metadata;

		// Token vault PDA is blacklisted
		ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT.into(),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Usdc),
			CcmValidityError::UnsupportedAsset
		);

		ccm.cf_parameters = SolCcmAccounts {
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
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Usdc),
			CcmValidityError::UnsupportedAsset
		);

		// Agg key is blacklisted
		let agg_key = MockEnv::current_agg_key().unwrap();
		ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress { pubkey: agg_key.into(), is_writable: true },
			remaining_accounts: vec![
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x01; 32]), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Usdc),
			CcmValidityError::UnsupportedAsset
		);

		ccm.cf_parameters = SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: crate::sol::SolPubkey([0x01; 32]),
				is_writable: true,
			},
			remaining_accounts: vec![
				SolCcmAddress { pubkey: agg_key.into(), is_writable: false },
				SolCcmAddress { pubkey: crate::sol::SolPubkey([0x02; 32]), is_writable: false },
			],
		}
		.encode()
		.try_into()
		.unwrap();
		assert_err!(
			SolanaCcmValidityChecker::<MockEnv>::is_valid(&ccm, cf_primitives::Asset::Usdc),
			CcmValidityError::UnsupportedAsset
		);
	}
}
