//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder is used to build single Solana Instruction used for Vault Swap.
//!
//! Such Instruction can be signed and sent to the Program on Solana directly to invoke
//! certain functions.

use codec::Encode;
use sol_prim::consts::{SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID};
use sp_core::ConstU32;

use crate::{
	address::EncodedAddress,
	cf_parameters::*,
	sol::{
		sol_tx_core::program_instructions::swap_endpoints::{
			SwapEndpointProgram, SwapNativeParams, SwapTokenParams,
		},
		SolAddress, SolAmount, SolApiEnvironment, SolInstruction, SolPubkey,
	},
	CcmChannelMetadata, ChannelRefundParametersDecoded,
};
use cf_primitives::{
	chains::assets::any::Asset, AccountId, AffiliateAndFee, BasisPoints, Beneficiary,
	DcaParameters, MAX_AFFILIATES,
};
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

fn system_program_id() -> SolAddress {
	SYSTEM_PROGRAM_ID
}

fn token_program_id() -> SolAddress {
	TOKEN_PROGRAM_ID
}

pub struct SolanaInstructionBuilder;

impl SolanaInstructionBuilder {
	pub fn x_swap_native(
		api_environment: SolApiEnvironment,
		agg_key: SolPubkey,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		broker_id: AccountId,
		broker_commission: BasisPoints,
		refund_parameters: ChannelRefundParametersDecoded,
		boost_fee: BasisPoints,
		affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
		dca_parameters: Option<DcaParameters>,
		from: SolPubkey,
		event_data_account: SolPubkey,
		input_amount: SolAmount,
		ccm: Option<CcmChannelMetadata>,
	) -> SolInstruction {
		let cf_parameters = Self::build_cf_parameters(
			refund_parameters,
			dca_parameters,
			boost_fee,
			broker_id,
			broker_commission,
			affiliate_fees,
			ccm.as_ref(),
		);

		SwapEndpointProgram::with_id(api_environment.swap_endpoint_program).x_swap_native(
			SwapNativeParams {
				amount: input_amount,
				dst_chain: destination_address.chain() as u32,
				dst_address: destination_address.into_vec(),
				dst_token: destination_asset as u32,
				ccm_parameters: ccm.map(|metadata| metadata.into()),
				cf_parameters,
			},
			api_environment.vault_program_data_account,
			agg_key,
			from,
			event_data_account,
			api_environment.swap_endpoint_program_data_account,
			system_program_id(),
		)
	}

	pub fn x_swap_usdc(
		api_environment: SolApiEnvironment,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		broker_id: AccountId,
		broker_commission: BasisPoints,
		refund_parameters: ChannelRefundParametersDecoded,
		boost_fee: BasisPoints,
		affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
		dca_parameters: Option<DcaParameters>,
		from: SolPubkey,
		from_token_account: SolPubkey,
		event_data_account: SolPubkey,
		token_supported_account: SolPubkey,
		input_amount: SolAmount,
		ccm: Option<CcmChannelMetadata>,
	) -> SolInstruction {
		let cf_parameters = Self::build_cf_parameters(
			refund_parameters,
			dca_parameters,
			boost_fee,
			broker_id,
			broker_commission,
			affiliate_fees,
			ccm.as_ref(),
		);

		SwapEndpointProgram::with_id(api_environment.swap_endpoint_program).x_swap_token(
			SwapTokenParams {
				amount: input_amount,
				dst_chain: destination_address.chain() as u32,
				dst_address: destination_address.into_vec(),
				dst_token: destination_asset as u32,
				ccm_parameters: ccm.map(|metadata| metadata.into()),
				cf_parameters,
				decimals: SOL_USDC_DECIMAL,
			},
			api_environment.vault_program_data_account,
			api_environment.usdc_token_vault_ata,
			from,
			from_token_account,
			event_data_account,
			api_environment.swap_endpoint_program_data_account,
			token_supported_account,
			token_program_id(),
			api_environment.usdc_token_mint_pubkey,
			system_program_id(),
		)
	}

	/// Builds the cf_parameter. The logic is shared between Sol and Usdc vault swap instruction.
	fn build_cf_parameters(
		refund_parameters: ChannelRefundParametersDecoded,
		dca_parameters: Option<DcaParameters>,
		boost_fee: BasisPoints,
		broker_id: AccountId,
		broker_commission: BasisPoints,
		affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
		ccm: Option<&CcmChannelMetadata>,
	) -> Vec<u8> {
		let vault_swap_parameters = VaultSwapParameters {
			refund_params: refund_parameters,
			dca_params: dca_parameters,
			boost_fee: boost_fee.try_into().unwrap_or(u8::MAX),
			broker_fee: Beneficiary { account: broker_id, bps: broker_commission },
			affiliate_fees,
		};

		match ccm {
			Some(ccm) => VersionedCcmCfParameters::V0(CfParameters {
				ccm_additional_data: ccm.ccm_additional_data.clone(),
				vault_swap_parameters,
			})
			.encode(),
			None => VersionedCfParameters::V0(CfParameters {
				ccm_additional_data: (),
				vault_swap_parameters,
			})
			.encode(),
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		sol::{
			signing_key::SolSigningKey, sol_tx_core::sol_test_values::*, SolAddress, SolHash,
			SolMessage, SolTransaction,
		},
		ForeignChainAddress,
	};
	use cf_primitives::{AffiliateShortId, DcaParameters};
	use sol_prim::consts::{const_address, MAX_TRANSACTION_LENGTH};

	// private key: ead22312d80f573924a27595271bd2ec0aa20a270587c8a399136166561ea58c
	const DESTINATION_ADDRESS_ETH: EncodedAddress =
		EncodedAddress::Eth(hex_literal::hex!("756FBdE9c71EaE05C2f7169f816b0Bd11D978020"));

	// Test Solana accounts. Generated by
	// ```rust
	// let key = SolSigningKey.new();
	// key.print_pub_and_private_keys();
	// ```

	//const DESTINATION_ADDRESS_KEY_BYTES: [u8; 32] = [242, 33, 23, 21, 58, 254, 23, 134, 199, 91,
	// 117, 2, 20, 116, 174, 15, 191, 69, 254, 42, 135, 88, 210, 88, 225, 158, 31, 184, 181, 50, 16,
	// 195];
	const DESTINATION_ADDRESS_SOL: SolAddress =
		const_address("BdyHK5DckpAFGcbZveGLPjjMEaADGfNeqcRXKoyd33kA");

	const FROM_KEY_BYTES: [u8; 32] = [
		130, 14, 62, 77, 129, 146, 185, 187, 159, 15, 165, 161, 93, 111, 249, 198, 145, 149, 193,
		229, 147, 69, 73, 190, 10, 208, 151, 131, 194, 205, 116, 232,
	];
	const FROM: SolAddress = const_address("EwgZksaPybTUyhcEMn3aR46HZokR4NH6d1Wy8d51qZ6G");

	const EVENT_DATA_ACCOUNT_KEY_BYTES: [u8; 32] = [
		133, 220, 70, 223, 197, 127, 106, 46, 178, 73, 164, 200, 88, 128, 97, 144, 20, 132, 211,
		34, 196, 159, 28, 118, 5, 209, 12, 245, 241, 223, 8, 67,
	];
	const EVENT_DATA_ACCOUNT: SolAddress =
		const_address("9acHwMGmeoMr5o8Cw1V2U4HjMQwhced3eQP31yYEhYDU");

	//const TOKEN_SUPPORTED_ACCOUNT_KEY_BYTES: [u8; 32] = [0, 81, 206, 126, 204, 53, 163, 79, 5,
	// 119, 184, 1, 97, 237, 114, 120, 23, 6, 227, 206, 239, 132, 130, 212, 241, 12, 21, 185, 66,
	// 252, 127, 8];
	const TOKEN_SUPPORTED_ACCOUNT: SolAddress =
		const_address("48vfkGMyVDUNq689aocfYQsoqubZjjme7cja21cbnMnK");

	const BROKER_COMMISSION: BasisPoints = 1u16;
	const BOOST_FEE: BasisPoints = 2u16;
	const INPUT_AMOUNT: SolAmount = 1_234_567_890u64;

	const BLOCKHASH: SolHash = SolHash([0x00; 32]);

	// Sisyphos broker account: 0xa622ebf634ff6cdafe1b7912d8699b34a4d9a08598af0b0c90eaf1e912de1f19
	fn broker_id() -> AccountId {
		AccountId::from(hex_literal::hex!(
			"a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e"
		))
	}

	fn channel_refund_parameters() -> ChannelRefundParametersDecoded {
		ChannelRefundParametersDecoded {
			min_price: sp_core::U256::default(),
			refund_address: ForeignChainAddress::Sol(DESTINATION_ADDRESS_SOL),
			retry_duration: 10u32,
		}
	}

	fn affiliate_and_fees() -> BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>> {
		vec![
			AffiliateAndFee { affiliate: AffiliateShortId(1u8), fee: 10u8 },
			AffiliateAndFee { affiliate: AffiliateShortId(2u8), fee: 20u8 },
		]
		.try_into()
		.unwrap()
	}

	fn dca_parameters() -> DcaParameters {
		DcaParameters { number_of_chunks: 10u32, chunk_interval: 20u32 }
	}

	fn into_transaction(instructions: SolInstruction, payer: SolPubkey) -> SolTransaction {
		// Build mock Transaction for testing.
		let transaction =
			SolTransaction::new_unsigned(SolMessage::new(&[instructions], Some(&payer)));

		let mock_serialized_tx = transaction
			.clone()
			.finalize_and_serialize()
			.expect("Transaction building must succeed.");

		assert!(
			mock_serialized_tx.len() < MAX_TRANSACTION_LENGTH,
			"Transaction exceeded max length"
		);

		transaction
	}

	#[test]
	fn can_build_x_swap_native_instruction_no_ccm() {
		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_native(
				api_env(),
				agg_key().into(),
				Asset::Eth,
				DESTINATION_ADDRESS_ETH,
				broker_id(),
				BROKER_COMMISSION,
				channel_refund_parameters(),
				BOOST_FEE,
				affiliate_and_fees(),
				Some(dca_parameters()),
				FROM.into(),
				EVENT_DATA_ACCOUNT.into(),
				INPUT_AMOUNT,
				None,
			),
			FROM.into(),
		);

		let expected_serialized_tx = hex_literal::hex!("020bc8ff4cf633fe3f4de509a7af709555457c561e54759030bb5016fe415906ebec56c093804504e4e23dc166338dd601ee4bab17c91438d5a0d37631ff8d4009932d9c088a56f8c06cbaec851a1f282e20c0f6087471bafe594af01e435f2fe80586c61e3e61bf6a5c10366d2adcc83e84be2165b49856494ba1246354dbac0202000307cf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc28317f799121d6c125f312c5f423a51959ce1d41df06af977e9a17f48b2c82ecf89f1c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d2f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb00000000000000000000000000000000000000000000000000000000000000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e091ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229380000000000000000000000000000000000000000000000000000000000000000010606050300010204ac01a3265ce2f3698dc4d2029649000000000100000014000000756fbde9c71eae05c2f7169f816b0bd11d978020010000000077000000000a000000049e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a0214").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();
		let event_data_account_signing_key =
			SolSigningKey::from_bytes(&EVENT_DATA_ACCOUNT_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key, event_data_account_signing_key].into(),
			BLOCKHASH.into(),
		);
	}

	#[test]
	fn can_build_x_swap_native_instruction_with_ccm() {
		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_native(
				api_env(),
				agg_key().into(),
				Asset::SolUsdc,
				EncodedAddress::Sol(DESTINATION_ADDRESS_SOL.0),
				broker_id(),
				BROKER_COMMISSION,
				channel_refund_parameters(),
				BOOST_FEE,
				affiliate_and_fees(),
				Some(dca_parameters()),
				FROM.into(),
				EVENT_DATA_ACCOUNT.into(),
				INPUT_AMOUNT,
				Some(ccm_parameter().channel_metadata),
			),
			FROM.into(),
		);

		let expected_serialized_tx = hex_literal::hex!("029fcaf3caad856ff9b088a0f68d6ac6f05f1b8686505b0313a8bfd713143ee368a9a60946e9f157094eaab17a42a4e8bd1dbbe682a20a2faffe8a71efaa60490c9d4dd4bb013c73df5cbf89015e5626d70725081f11e6b3200ea1cd857e31561061b7bde75933021436ffe1fccb57de6b80ae9b007dde3abd7da54ef06c75e20d02000307cf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc28317f799121d6c125f312c5f423a51959ce1d41df06af977e9a17f48b2c82ecf89f1c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d2f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb00000000000000000000000000000000000000000000000000000000000000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e091ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229380000000000000000000000000000000000000000000000000000000000000000010606050300010204ad02a3265ce2f3698dc4d20296490000000005000000200000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0a00000001040000007c1d0f070000000000000000dc000000008d017417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed480104a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e50090e0b0f5b60147b325842c1fc68f6c90fe26419ea7c4afeb982f71f1f54b5b440a000000049e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a0214").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();
		let event_data_account_signing_key =
			SolSigningKey::from_bytes(&EVENT_DATA_ACCOUNT_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key, event_data_account_signing_key].into(),
			BLOCKHASH.into(),
		);
	}

	#[test]
	fn can_build_x_swap_token_instruction_no_ccm() {
		let from_usdc_account =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				FROM,
				api_env().usdc_token_mint_pubkey,
			)
			.unwrap()
			.address
			.into();

		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_usdc(
				api_env(),
				Asset::Eth,
				DESTINATION_ADDRESS_ETH,
				broker_id(),
				BROKER_COMMISSION,
				channel_refund_parameters(),
				BOOST_FEE,
				affiliate_and_fees(),
				Some(dca_parameters()),
				FROM.into(),
				from_usdc_account,
				EVENT_DATA_ACCOUNT.into(),
				TOKEN_SUPPORTED_ACCOUNT.into(),
				INPUT_AMOUNT,
				None,
			),
			FROM.into(),
		);

		let expected_serialized_tx = hex_literal::hex!("02300ff413fa335f3a24300f563cf85cb7ccc53aaa2c0c3180615dde6f1d11dcf68d1ee32c5338915fa45d918a428175ca6393d98e9cdcf2fca676bd89d03d4603549c691d2941934004581d7e4bd102a1db151997d2ee7687291a547dbbdcdb8d8d657991e9e3db587660f6fb004650cebd01440693048a8d164144a3dd16600f0200060bcf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc28317f799121d6c125f312c5f423a51959ce1d41df06af977e9a17f48b2c82ecf89f1c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d227fefc7ec198ef3eacb0f17871e1fad81f07a40cd55f4f364c3915877d89bd8ae91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada000000000000000000000000000000000000000000000000000000000000000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee871ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229382e9acf1ff8568fbe655e616a167591aeedc250afbc88d759ec959b1982e8769c000000000000000000000000000000000000000000000000000000000000000001090a0704000301020a060805ad014532fc63e55377ebd2029649000000000100000014000000756fbde9c71eae05c2f7169f816b0bd11d978020010000000077000000000a000000049e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a021406").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();
		let event_data_account_signing_key =
			SolSigningKey::from_bytes(&EVENT_DATA_ACCOUNT_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key, event_data_account_signing_key].into(),
			BLOCKHASH.into(),
		);
	}

	#[test]
	fn can_build_x_swap_token_instruction_with_ccm() {
		let from_usdc_account =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				FROM,
				api_env().usdc_token_mint_pubkey,
			)
			.unwrap()
			.address
			.into();

		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_usdc(
				api_env(),
				Asset::Sol,
				EncodedAddress::Sol(DESTINATION_ADDRESS_SOL.0),
				broker_id(),
				BROKER_COMMISSION,
				channel_refund_parameters(),
				BOOST_FEE,
				affiliate_and_fees(),
				Some(dca_parameters()),
				FROM.into(),
				from_usdc_account,
				EVENT_DATA_ACCOUNT.into(),
				TOKEN_SUPPORTED_ACCOUNT.into(),
				INPUT_AMOUNT,
				Some(ccm_parameter().channel_metadata),
			),
			FROM.into(),
		);

		let expected_serialized_tx = hex_literal::hex!("02c590afa4ba0290e742eae063bdd0b7abd8c6a889cd9032cb354d02c806ef0b665c81c687f7ea7c28c65ac7f9027b3404cfbc39fe1dc0e62b364c96d0707a5f026d23a30e59363b1698a35972703110ba00c81798762c57d4f011adc2be7996d1b2d68e1ca7ac119c4e6779dbcac83ceb702cb4a5c14dbca1a3c7f2d2d2114d000200060bcf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc28317f799121d6c125f312c5f423a51959ce1d41df06af977e9a17f48b2c82ecf89f1c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d227fefc7ec198ef3eacb0f17871e1fad81f07a40cd55f4f364c3915877d89bd8ae91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada000000000000000000000000000000000000000000000000000000000000000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee871ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229382e9acf1ff8568fbe655e616a167591aeedc250afbc88d759ec959b1982e8769c000000000000000000000000000000000000000000000000000000000000000001090a0704000301020a060805ae024532fc63e55377ebd20296490000000005000000200000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0900000001040000007c1d0f070000000000000000dc000000008d017417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed480104a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e50090e0b0f5b60147b325842c1fc68f6c90fe26419ea7c4afeb982f71f1f54b5b440a000000049e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a021406").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();
		let event_data_account_signing_key =
			SolSigningKey::from_bytes(&EVENT_DATA_ACCOUNT_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key, event_data_account_signing_key].into(),
			BLOCKHASH.into(),
		);
	}
}
