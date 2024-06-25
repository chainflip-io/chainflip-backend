use std::{fmt, str::FromStr, sync::Arc};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use cf_chains::{
	address::EncodedAddress, dot::PolkadotAccountId, evm::to_evm_address, sol::SolAddress,
	AnyChain, CcmChannelMetadata, ChannelRefundParameters, ForeignChain,
};
pub use cf_primitives::{AccountRole, Affiliates, Asset, BasisPoints, ChannelId, SemVer};
use futures::FutureExt;
use pallet_cf_account_roles::MAX_LENGTH_FOR_VANITY_NAME;
use pallet_cf_governance::ExecutionMode;
use serde::{Deserialize, Serialize};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
pub use sp_core::crypto::AccountId32;
use sp_core::{ed25519::Public as EdPublic, sr25519::Public as SrPublic, Bytes, Pair, H256, U256};
pub use state_chain_runtime::chainflip::BlockUpdate;
use state_chain_runtime::{opaque::SessionKeys, RuntimeCall};
use zeroize::Zeroize;
pub mod primitives {
	pub use cf_primitives::*;
	pub use pallet_cf_governance::ProposalId;
	pub use state_chain_runtime::{self, BlockNumber, Hash};
	pub type RedemptionAmount = pallet_cf_funding::RedemptionAmount<FlipBalance>;
	pub use cf_chains::{
		address::{EncodedAddress, ForeignChainAddress},
		CcmChannelMetadata, CcmDepositMetadata, ChannelRefundParameters,
	};
}
pub use cf_chains::eth::Address as EthereumAddress;
pub use chainflip_engine::{
	settings,
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcApi, RawRpcApi},
		chain_api::ChainApi,
		extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized, WaitFor, WaitForResult},
		storage_api::StorageApi,
		BlockInfo,
	},
};

pub mod lp;
pub mod queries;

pub use chainflip_node::chain_spec::use_chainflip_account_id_encoding;

use chainflip_engine::state_chain_observer::client::{
	base_rpc_api::BaseRpcClient, extrinsic_api::signed::UntilInBlock, DefaultRpcClient,
	StateChainClient,
};
use utilities::{clean_hex_address, task_scope::Scope};

lazy_static::lazy_static! {
	static ref API_VERSION: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
	};
}

#[async_trait]
pub trait AuctionPhaseApi {
	async fn is_auction_phase(&self) -> Result<bool>;
}

#[async_trait]
impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: Send + Sync + 'static,
	> AuctionPhaseApi for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	async fn is_auction_phase(&self) -> Result<bool> {
		Ok(self.base_rpc_client.raw_rpc_client.cf_is_auction_phase(None).await?)
	}
}

#[async_trait]
pub trait RotateSessionKeysApi {
	async fn rotate_session_keys(&self) -> Result<Bytes>;
}

#[async_trait]
impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: Send + Sync + 'static,
	> RotateSessionKeysApi for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	async fn rotate_session_keys(&self) -> Result<Bytes> {
		Ok(self.base_rpc_client.raw_rpc_client.rotate_keys().await?)
	}
}

pub async fn request_block(
	block_hash: state_chain_runtime::Hash,
	state_chain_settings: &settings::StateChain,
) -> Result<state_chain_runtime::SignedBlock> {
	println!("Querying the state chain for the block with hash {block_hash:x?}.");

	DefaultRpcClient::connect(&state_chain_settings.ws_endpoint)
		.await?
		.block(block_hash)
		.await?
		.ok_or_else(|| anyhow!("unknown block hash"))
}

pub struct StateChainApi {
	pub state_chain_client: Arc<StateChainClient>,
}

impl StateChainApi {
	pub async fn connect<'a>(
		scope: &Scope<'a, anyhow::Error>,
		state_chain_settings: settings::StateChain,
	) -> Result<Self, anyhow::Error> {
		let (.., state_chain_client) = StateChainClient::connect_with_account(
			scope,
			&state_chain_settings.ws_endpoint,
			&state_chain_settings.signing_key_file,
			AccountRole::Unregistered,
			false,
			false,
			None,
		)
		.await?;

		Ok(Self { state_chain_client })
	}

	pub fn operator_api(&self) -> Arc<impl OperatorApi> {
		self.state_chain_client.clone()
	}

	pub fn governance_api(&self) -> Arc<impl GovernanceApi> {
		self.state_chain_client.clone()
	}

	pub fn validator_api(&self) -> Arc<impl ValidatorApi> {
		self.state_chain_client.clone()
	}

	pub fn broker_api(&self) -> Arc<impl BrokerApi> {
		self.state_chain_client.clone()
	}

	pub fn lp_api(&self) -> Arc<impl lp::LpApi> {
		self.state_chain_client.clone()
	}

	pub fn query_api(&self) -> queries::QueryApi {
		queries::QueryApi { state_chain_client: self.state_chain_client.clone() }
	}
}

#[async_trait]
impl GovernanceApi for StateChainClient {}
#[async_trait]
impl BrokerApi for StateChainClient {}
#[async_trait]
impl OperatorApi for StateChainClient {}
#[async_trait]
impl ValidatorApi for StateChainClient {}

#[async_trait]
pub trait ValidatorApi: SimpleSubmissionApi {
	async fn register_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_validator::Call::register_as_validator {})
			.await
	}
	async fn deregister_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_validator::Call::deregister_as_validator {})
			.await
	}
	async fn stop_bidding(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_validator::Call::stop_bidding {})
			.await
	}
	async fn start_bidding(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_validator::Call::start_bidding {})
			.await
	}
}

#[async_trait]
pub trait OperatorApi: SignedExtrinsicApi + RotateSessionKeysApi + AuctionPhaseApi {
	async fn request_redemption(
		&self,
		amount: primitives::RedemptionAmount,
		address: EthereumAddress,
		executor: Option<EthereumAddress>,
	) -> Result<H256> {
		let call = RuntimeCall::from(pallet_cf_funding::Call::redeem { amount, address, executor });

		let (tx_hash, ..) =
			self.submit_signed_extrinsic_with_dry_run(call).await?.until_in_block().await?;

		Ok(tx_hash)
	}

	async fn bind_redeem_address(&self, address: EthereumAddress) -> Result<H256> {
		let (tx_hash, ..) = self
			.submit_signed_extrinsic(pallet_cf_funding::Call::bind_redeem_address { address })
			.await
			.until_in_block()
			.await?;

		Ok(tx_hash)
	}

	async fn bind_executor_address(&self, executor_address: EthereumAddress) -> Result<H256> {
		let (tx_hash, ..) = self
			.submit_signed_extrinsic(pallet_cf_funding::Call::bind_executor_address {
				executor_address,
			})
			.await
			.until_finalized()
			.await?;

		Ok(tx_hash)
	}

	async fn register_account_role(&self, role: AccountRole) -> Result<H256> {
		let call = match role {
			AccountRole::Validator =>
				RuntimeCall::from(pallet_cf_validator::Call::register_as_validator {}),
			AccountRole::Broker =>
				RuntimeCall::from(pallet_cf_swapping::Call::register_as_broker {}),
			AccountRole::LiquidityProvider =>
				RuntimeCall::from(pallet_cf_lp::Call::register_lp_account {}),
			AccountRole::Unregistered => bail!("Cannot register account role {:?}", role),
		};

		let (tx_hash, ..) =
			self.submit_signed_extrinsic_with_dry_run(call).await?.until_in_block().await?;
		Ok(tx_hash)
	}

	async fn rotate_session_keys(&self) -> Result<H256> {
		let raw_keys = RotateSessionKeysApi::rotate_session_keys(self).await?;

		let aura_key: [u8; 32] = raw_keys[0..32].try_into().unwrap();
		let grandpa_key: [u8; 32] = raw_keys[32..64].try_into().unwrap();

		let (tx_hash, ..) = self
			.submit_signed_extrinsic(pallet_cf_validator::Call::set_keys {
				keys: SessionKeys {
					aura: AuraId::from(SrPublic::from_raw(aura_key)),
					grandpa: GrandpaId::from(EdPublic::from_raw(grandpa_key)),
				},
				proof: [0; 1].to_vec(),
			})
			.await
			.until_in_block()
			.await?;

		Ok(tx_hash)
	}

	async fn set_vanity_name(&self, name: String) -> Result<()> {
		let (tx_hash, ..) = self
			.submit_signed_extrinsic(pallet_cf_account_roles::Call::set_vanity_name {
				name: name.into_bytes().try_into().or_else(|_| {
					bail!("Name too long. Max length is {} characters.", MAX_LENGTH_FOR_VANITY_NAME,)
				})?,
			})
			.await
			.until_in_block()
			.await?;
		println!("Vanity name set at tx {tx_hash:#x}.");
		Ok(())
	}
}

#[async_trait]
pub trait GovernanceApi: SignedExtrinsicApi {
	async fn force_rotation(&self) -> Result<()> {
		println!("Submitting governance proposal for rotation.");
		self.submit_signed_extrinsic(pallet_cf_governance::Call::propose_governance_extrinsic {
			call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			execution: ExecutionMode::Automatic,
		})
		.await
		.until_in_block()
		.await?;

		println!("If you're the governance dictator, the rotation will begin soon.");

		Ok(())
	}
}

pub struct SwapDepositAddress {
	pub address: String,
	pub issued_block: state_chain_runtime::BlockNumber,
	pub channel_id: ChannelId,
	pub source_chain_expiry_block: <AnyChain as cf_chains::Chain>::ChainBlockNumber,
	pub channel_opening_fee: U256,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WithdrawFeesDetail {
	pub tx_hash: H256,
	pub egress_id: (ForeignChain, u64),
	pub egress_amount: U256,
	pub egress_fee: U256,
	pub destination_address: String,
}

impl fmt::Display for WithdrawFeesDetail {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"\
			Tx hash: {:?}\n\
			Egress id: {:?}\n\
			Egress amount: {}\n\
			Egress fee: {}\n\
			Destination address: {}\n\
			",
			self.tx_hash,
			self.egress_id,
			self.egress_amount,
			self.egress_fee,
			self.destination_address,
		)
	}
}

#[async_trait]
pub trait BrokerApi: SignedExtrinsicApi + Sized + Send + Sync + 'static {
	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		broker_commission: BasisPoints,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Affiliates<AccountId32>,
		refund_parameters: Option<ChannelRefundParameters>,
	) -> Result<SwapDepositAddress> {
		let (_tx_hash, events, header, ..) = self
			.submit_signed_extrinsic_with_dry_run(if affiliate_fees.is_empty() {
				pallet_cf_swapping::Call::request_swap_deposit_address {
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					channel_metadata,
					boost_fee: boost_fee.unwrap_or_default(),
				}
			} else {
				pallet_cf_swapping::Call::request_swap_deposit_address_with_affiliates {
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					channel_metadata,
					boost_fee: boost_fee.unwrap_or_default(),
					affiliate_fees,
					refund_parameters,
				}
			})
			.await?
			.until_in_block()
			.await?;

		if let Some(state_chain_runtime::RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::SwapDepositAddressReady {
				deposit_address,
				channel_id,
				source_chain_expiry_block,
				channel_opening_fee,
				..
			},
		)) = events.iter().find(|event| {
			matches!(
				event,
				state_chain_runtime::RuntimeEvent::Swapping(
					pallet_cf_swapping::Event::SwapDepositAddressReady { .. }
				)
			)
		}) {
			Ok(SwapDepositAddress {
				address: deposit_address.to_string(),
				issued_block: header.number,
				channel_id: *channel_id,
				source_chain_expiry_block: *source_chain_expiry_block,
				channel_opening_fee: (*channel_opening_fee).into(),
			})
		} else {
			bail!("No SwapDepositAddressReady event was found");
		}
	}
	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: EncodedAddress,
	) -> Result<WithdrawFeesDetail> {
		let (tx_hash, events, ..) = self
			.submit_signed_extrinsic(RuntimeCall::from(pallet_cf_swapping::Call::withdraw {
				asset,
				destination_address,
			}))
			.await
			.until_in_block()
			.await?;

		if let Some(state_chain_runtime::RuntimeEvent::Swapping(
			pallet_cf_swapping::Event::WithdrawalRequested {
				egress_amount,
				egress_fee,
				destination_address,
				egress_id,
				..
			},
		)) = events.iter().find(|event| {
			matches!(
				event,
				state_chain_runtime::RuntimeEvent::Swapping(
					pallet_cf_swapping::Event::WithdrawalRequested { .. }
				)
			)
		}) {
			Ok(WithdrawFeesDetail {
				tx_hash,
				egress_id: *egress_id,
				egress_amount: (*egress_amount).into(),
				egress_fee: (*egress_fee).into(),
				destination_address: destination_address.to_string(),
			})
		} else {
			bail!("No WithdrawalRequested event was found");
		}
	}
	async fn register_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_swapping::Call::register_as_broker {})
			.await
	}
	async fn deregister_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_swapping::Call::deregister_as_broker {})
			.await
	}
}

#[async_trait]
pub trait SimpleSubmissionApi: SignedExtrinsicApi {
	async fn simple_submission_with_dry_run<C>(&self, call: C) -> Result<H256>
	where
		C: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static,
	{
		let (tx_hash, ..) =
			self.submit_signed_extrinsic_with_dry_run(call).await?.until_in_block().await?;
		Ok(tx_hash)
	}
}

#[async_trait]
impl<T: SignedExtrinsicApi + Sized + Send + Sync + 'static> SimpleSubmissionApi for T {}

/// Sanitize the given address (hex or base58) and turn it into a EncodedAddress of the given
/// chain.
pub fn clean_foreign_chain_address(chain: ForeignChain, address: &str) -> Result<EncodedAddress> {
	Ok(match chain {
		ForeignChain::Ethereum => EncodedAddress::Eth(clean_hex_address(address)?),
		ForeignChain::Polkadot =>
			EncodedAddress::Dot(PolkadotAccountId::from_str(address).map(|id| *id.aliased_ref())?),
		ForeignChain::Bitcoin => EncodedAddress::Btc(address.as_bytes().to_vec()),
		ForeignChain::Arbitrum => EncodedAddress::Arb(clean_hex_address(address)?),
		ForeignChain::Solana => EncodedAddress::Sol(SolAddress::from_str(address)?.into()),
	})
}

#[derive(Debug, Zeroize, PartialEq, Eq)]
/// Public and Secret keys as bytes
pub struct KeyPair {
	pub secret_key: Vec<u8>,
	pub public_key: Vec<u8>,
}

// Serialize the keypair as hex strings for convenience
impl Serialize for KeyPair {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		use serde::ser::SerializeStruct;

		let secret_key_hex = hex::encode(&self.secret_key);
		let public_key_hex = hex::encode(&self.public_key);
		let mut state = serializer.serialize_struct("KeyPair", 2)?;
		state.serialize_field("secret_key", &secret_key_hex)?;
		state.serialize_field("public_key", &public_key_hex)?;
		state.end()
	}
}

/// Generate a new random node key.
///
/// This key is used for secure communication between Validators.
pub fn generate_node_key() -> Result<(KeyPair, libp2p_identity::PeerId)> {
	let signing_keypair = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
	let libp2p_keypair = libp2p_identity::Keypair::ed25519_from_bytes(signing_keypair.to_bytes())?;

	Ok((
		KeyPair {
			secret_key: signing_keypair.to_bytes().to_vec(),
			public_key: signing_keypair.verifying_key().to_bytes().to_vec(),
		},
		libp2p_keypair.public().to_peer_id(),
	))
}

/// Generate a signing key (aka. account key).
///
/// If no seed phrase is provided, a new random seed phrase will be created.
pub fn generate_signing_key(seed_phrase: Option<&str>) -> Result<(String, KeyPair, AccountId32)> {
	use bip39::{Language, Mnemonic, MnemonicType};

	let mnemonic = seed_phrase
		.map(|phrase| Mnemonic::from_phrase(phrase, Language::English))
		.unwrap_or_else(|| Ok(Mnemonic::new(MnemonicType::Words12, Language::English)))?;

	sp_core::Pair::from_phrase(mnemonic.phrase(), None)
		.map(|(pair, seed)| {
			let pair: sp_core::sr25519::Pair = pair;
			(
				mnemonic.to_string(),
				KeyPair { secret_key: seed.to_vec(), public_key: pair.public().to_vec() },
				pair.public().into(),
			)
		})
		.map_err(|e| anyhow!("Failed to generate signing key - invalid seed phrase. Error: {e:?}"))
}

/// Generate an ethereum key.
///
/// A chainflip validator must have their own Ethereum private keys and be capable of submitting
/// transactions.
///
/// If no seed phrase is provided, a new random seed phrase will be created.
///
/// Note this is *not* a general-purpose utility for deriving Ethereum addresses. You should
/// not expect to be able to recover this address in any mainstream wallet. Notably, this
/// does *not* use BIP44 derivation paths.
pub fn generate_ethereum_key(
	seed_phrase: Option<&str>,
) -> Result<(String, KeyPair, EthereumAddress)> {
	use bip39::{Language, Mnemonic, MnemonicType, Seed};

	let mnemonic = seed_phrase
		.map(|phrase| Mnemonic::from_phrase(phrase, Language::English))
		.unwrap_or_else(|| Ok(Mnemonic::new(MnemonicType::Words12, Language::English)))?;

	let seed = Seed::new(&mnemonic, Default::default());
	let master_key_bytes = hmac_sha512::HMAC::mac(seed, b"Chainflip Ethereum Key");

	let secret_key = libsecp256k1::SecretKey::parse_slice(&master_key_bytes[..32])
		.context("Unable to derive secret key.")?;
	let public_key = libsecp256k1::PublicKey::from_secret_key(&secret_key);

	Ok((
		mnemonic.to_string(),
		KeyPair {
			secret_key: secret_key.serialize().to_vec(),
			public_key: public_key.serialize_compressed().to_vec(),
		},
		to_evm_address(public_key),
	))
}

#[cfg(test)]
mod tests {
	use super::*;

	mod key_generation {

		use super::*;
		use sp_core::crypto::Ss58Codec;

		#[test]
		fn restored_keys_remain_compatible() {
			const SEED_PHRASE: &str =
		"essay awesome afraid movie wish save genius eyebrow tonight milk agree pretty alcohol three whale";

			let generated = generate_signing_key(Some(SEED_PHRASE)).unwrap();

			// Compare the generated secret key with a known secret key generated using the
			// `chainflip-node key generate` command
			assert_eq!(
				"afabf42a9a99910cdd64795ef05ed71acfa2238f5682d26ae62028df3cc59727",
				hex::encode(generated.1.secret_key)
			);
			assert_eq!(
				(generated.0, generated.2),
				(
					SEED_PHRASE.to_string(),
					AccountId32::from_ss58check(
						"cFMziohdyxVZy4DGXw2zkapubUoTaqjvAM7QGcpyLo9Cba7HA"
					)
					.unwrap(),
				)
			);

			let generated = generate_ethereum_key(Some(SEED_PHRASE)).unwrap();
			assert_eq!(
				"5c25d9ae0363ecd8dd18da1608ead2a4dc1ec658d6ed412d47e10d486ff0d1db",
				hex::encode(generated.1.secret_key)
			);
			assert_eq!(
				(generated.0, generated.2.as_bytes().to_vec()),
				(
					SEED_PHRASE.to_string(),
					hex::decode("e01156ca92d904cc67ff47517bf3a3500b418280").unwrap()
				)
			);
		}

		#[test]
		fn test_restore_signing_keys() {
			let ref original @ (ref seed_phrase, ..) = generate_signing_key(None).unwrap();
			let restored = generate_signing_key(Some(seed_phrase)).unwrap();

			assert_eq!(*original, restored);
		}

		#[test]
		fn test_restore_eth_keys() {
			let ref original @ (ref seed_phrase, ..) = generate_ethereum_key(None).unwrap();
			let restored = generate_ethereum_key(Some(seed_phrase)).unwrap();

			assert_eq!(*original, restored);
		}

		#[test]
		fn test_dot_address_decoding() {
			assert_eq!(
				clean_foreign_chain_address(
					ForeignChain::Polkadot,
					"126PaS7kDWTdtiojd556gD4ZPcxj7KbjrMJj7xZ5i6XKfARE"
				)
				.unwrap(),
				clean_foreign_chain_address(
					ForeignChain::Polkadot,
					"0x305875a3025d8be7f7048a280aba2bd571126fc171986adc1af58d1f4e02f15e"
				)
				.unwrap(),
			);
			assert_eq!(
				clean_foreign_chain_address(
					ForeignChain::Polkadot,
					"126PaS7kDWTdtiojd556gD4ZPcxj7KbjrMJj7xZ5i6XKfARF"
				)
				.unwrap_err()
				.to_string(),
				anyhow!("Address is neither valid ss58: 'Invalid checksum' nor hex: 'Invalid character 'P' at position 3'").to_string(),
			);
		}
	}
}
