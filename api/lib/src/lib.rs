use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use cf_chains::{
	address::EncodedAddress, eth::to_ethereum_address, CcmDepositMetadata, ForeignChain,
};
use cf_primitives::{AccountRole, Asset, BasisPoints};
use futures::FutureExt;
use pallet_cf_validator::MAX_LENGTH_FOR_VANITY_NAME;
use serde::{Deserialize, Serialize};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{ed25519::Public as EdPublic, sr25519::Public as SrPublic, Bytes, Pair, H256};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::{opaque::SessionKeys, RuntimeCall};
use zeroize::Zeroize;

pub use sp_core::crypto::AccountId32;
pub mod primitives {
	pub use cf_primitives::*;
	pub use pallet_cf_governance::ProposalId;
	pub use state_chain_runtime::Hash;
	pub type RedemptionAmount = pallet_cf_funding::RedemptionAmount<FlipBalance>;
	pub use cf_chains::{
		address::{EncodedAddress, ForeignChainAddress},
		CcmDepositMetadata,
	};
}

pub mod lp;

pub use chainflip_engine::settings;
pub use chainflip_node::chain_spec::use_chainflip_account_id_encoding;

use chainflip_engine::state_chain_observer::client::{
	base_rpc_api::{BaseRpcApi, BaseRpcClient, RawRpcApi},
	extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
	DefaultRpcClient, StateChainClient,
};
use utilities::{clean_dot_address, clean_eth_address, task_scope::task_scope};

#[async_trait]
trait AuctionPhaseApi {
	async fn is_auction_phase(&self) -> Result<bool>;
}

#[async_trait]
impl<
		RawRpcClient: RawRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: Send + Sync + 'static,
	> AuctionPhaseApi for StateChainClient<SignedExtrinsicClient, BaseRpcClient<RawRpcClient>>
{
	async fn is_auction_phase(&self) -> Result<bool> {
		self.base_rpc_client
			.raw_rpc_client
			.cf_is_auction_phase(None)
			.await
			.map_err(Into::into)
	}
}

#[async_trait]
trait RotateSessionKeysApi {
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

async fn connect_submit_and_get_events<Call>(
	state_chain_settings: &settings::StateChain,
	call: Call,
	required_role: AccountRole,
) -> Result<(Vec<state_chain_runtime::RuntimeEvent>, state_chain_runtime::BlockNumber)>
where
	Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static,
{
	task_scope(|scope| {
		async {
			let (_state_chain_stream, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				required_role,
				false,
			)
			.await?;

			let (_tx_hash, events, header, ..) =
				state_chain_client.submit_signed_extrinsic(call).await.until_finalized().await?;

			Ok((events, header.number))
		}
		.boxed()
	})
	.await
}

pub async fn request_redemption(
	amount: primitives::RedemptionAmount,
	eth_address: [u8; 20],
	state_chain_settings: &settings::StateChain,
) -> Result<H256> {
	task_scope(|scope| {
		async {
			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::None,
				false,
			)
			.await?;

			// Are we in a current auction phase
			if state_chain_client.is_auction_phase().await? {
				bail!("We are currently in an auction phase. Please wait until the auction phase is over.");
			}

			let (tx_hash, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_funding::Call::redeem {
					amount,
					address: eth_address,
				})
				.await
				.until_finalized()
				.await?;

			Ok(tx_hash)
		}
		.boxed()
	})
	.await
}

pub async fn register_account_role(
	role: AccountRole,
	state_chain_settings: &settings::StateChain,
) -> Result<H256> {
	task_scope(|scope| {
		async {
			let call = match role {
				AccountRole::Validator =>
					RuntimeCall::from(pallet_cf_validator::Call::register_as_validator {}),
				AccountRole::Broker =>
					RuntimeCall::from(pallet_cf_swapping::Call::register_as_broker {}),
				AccountRole::LiquidityProvider =>
					RuntimeCall::from(pallet_cf_lp::Call::register_lp_account {}),
				AccountRole::None => bail!("Cannot register account role None"),
			};

			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::None,
				false,
			)
			.await?;

			let (tx_hash, ..) = state_chain_client
				.submit_signed_extrinsic(call)
				.await
				.until_finalized()
				.await
				.context("Could not register account role for account")?;
			Ok(tx_hash)
		}
		.boxed()
	})
	.await
}

pub async fn rotate_keys(state_chain_settings: &settings::StateChain) -> Result<H256> {
	task_scope(|scope| {
		async {
			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::None,
				false,
			)
			.await?;
			let seed = state_chain_client
				.rotate_session_keys()
				.await
				.context("Could not rotate session keys.")?;

			let aura_key: [u8; 32] = seed[0..32].try_into().unwrap();
			let grandpa_key: [u8; 32] = seed[32..64].try_into().unwrap();

			let new_session_key = SessionKeys {
				aura: AuraId::from(SrPublic::from_raw(aura_key)),
				grandpa: GrandpaId::from(EdPublic::from_raw(grandpa_key)),
			};

			let (tx_hash, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_validator::Call::set_keys {
					keys: new_session_key,
					proof: [0; 1].to_vec(),
				})
				.await
				.until_finalized()
				.await?;

			Ok(tx_hash)
		}
		.boxed()
	})
	.await
}

// Account must be the governance dictator in order for this to work.
pub async fn force_rotation(state_chain_settings: &settings::StateChain) -> Result<()> {
	task_scope(|scope| {
		async {
			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::None,
				false,
			)
			.await?;

			println!("Submitting governance proposal for rotation.");
			state_chain_client
				.submit_signed_extrinsic(pallet_cf_governance::Call::propose_governance_extrinsic {
					call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
				})
				.await
				.until_finalized()
				.await
				.context("Failed to submit rotation governance proposal")?;

			println!("If you're the governance dictator, the rotation will begin soon.");

			Ok(())
		}
		.boxed()
	})
	.await
}

pub async fn stop_bidding(state_chain_settings: &settings::StateChain) -> Result<()> {
	task_scope(|scope| {
		async {
			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::Validator,
				false,
			)
			.await?;
			let (tx_hash, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_funding::Call::stop_bidding {})
				.await
				.until_finalized()
				.await
				.context("Could not stop bidding")?;
			println!("Account stopped bidding, in tx {tx_hash:#x}.");
			Ok(())
		}
		.boxed()
	})
	.await
}

pub async fn start_bidding(state_chain_settings: &settings::StateChain) -> Result<()> {
	task_scope(|scope| {
		async {
			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::Validator,
				false,
			)
			.await?;

			let (tx_hash, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_funding::Call::start_bidding {})
				.await
				.until_finalized()
				.await
				.context("Could not start bidding")?;
			println!("Account started bidding at tx {tx_hash:#x}.");

			Ok(())
		}
		.boxed()
	})
	.await
}

pub async fn set_vanity_name(
	name: String,
	state_chain_settings: &settings::StateChain,
) -> Result<()> {
	task_scope(|scope| {
		async {
			if name.len() > MAX_LENGTH_FOR_VANITY_NAME {
				bail!("Name too long. Max length is {} characters.", MAX_LENGTH_FOR_VANITY_NAME,);
			}

			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::None,
				false,
			)
			.await?;
			let (tx_hash, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_validator::Call::set_vanity_name {
					name: name.as_bytes().to_vec(),
				})
				.await
				.until_finalized()
				.await
				.context("Could not set vanity name for your account")?;
			println!("Vanity name set at tx {tx_hash:#x}.");
			Ok(())
		}
		.boxed()
	})
	.await
}

#[derive(Serialize, Deserialize)]
pub struct SwapDepositAddress {
	pub address: String,
	pub expiry_block: state_chain_runtime::BlockNumber,
	pub issued_block: state_chain_runtime::BlockNumber,
}

pub async fn request_swap_deposit_address(
	state_chain_settings: &settings::StateChain,
	source_asset: Asset,
	destination_asset: Asset,
	destination_address: EncodedAddress,
	broker_commission_bps: BasisPoints,
	message_metadata: Option<CcmDepositMetadata>,
) -> Result<SwapDepositAddress> {
	let (events, block_number) = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_swapping::Call::request_swap_deposit_address {
			source_asset,
			destination_asset,
			destination_address,
			broker_commission_bps,
			message_metadata,
		},
		AccountRole::None,
	)
	.await?;

	if let Some(state_chain_runtime::RuntimeEvent::Swapping(
		pallet_cf_swapping::Event::SwapDepositAddressReady {
			deposit_address, expiry_block, ..
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
			expiry_block: *expiry_block,
			issued_block: block_number,
		})
	} else {
		panic!("SwapDepositAddressReady must have been generated");
	}
}

/// Sanitize the given address (hex or base58) and turn it into a EncodedAddress of the given
/// chain.
pub fn clean_foreign_chain_address(chain: ForeignChain, address: &str) -> Result<EncodedAddress> {
	Ok(match chain {
		ForeignChain::Ethereum =>
			EncodedAddress::Eth(clean_eth_address(address).map_err(anyhow::Error::msg)?),
		ForeignChain::Polkadot =>
			EncodedAddress::Dot(clean_dot_address(address).map_err(anyhow::Error::msg)?),
		ForeignChain::Bitcoin => EncodedAddress::Btc(address.as_bytes().to_vec()),
	})
}

#[derive(Debug, Zeroize)]
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
/// This key is used for secure communication between Validators.
pub fn generate_node_key() -> KeyPair {
	use rand_v7::SeedableRng;

	let mut rng = rand_v7::rngs::StdRng::from_entropy();
	let keypair = ed25519_dalek::Keypair::generate(&mut rng);

	KeyPair {
		secret_key: keypair.secret.as_bytes().to_vec(),
		public_key: keypair.public.to_bytes().to_vec(),
	}
}

/// Generate a signing key (aka validator key) using the seed phrase.
/// If no seed phrase is provided, a new random seed phrase will be created.
/// Returns the key, seed phrase and the derived account id.
pub fn generate_signing_key(seed_phrase: Option<&str>) -> Result<(KeyPair, String, AccountId32)> {
	use bip39::{Language, Mnemonic, MnemonicType};

	// Get a new random seed phrase if one was not provided
	let mnemonic = Mnemonic::new(MnemonicType::Words12, Language::English);
	let seed_phrase = seed_phrase.unwrap_or_else(|| mnemonic.phrase());

	sp_core::Pair::from_phrase(seed_phrase, None)
		.map(|(pair, seed)| {
			let pair: sp_core::sr25519::Pair = pair;
			(
				KeyPair { secret_key: seed.to_vec(), public_key: pair.public().to_vec() },
				seed_phrase.to_string(),
				pair.public().into(),
			)
		})
		.map_err(|_| anyhow::Error::msg("Invalid seed phrase"))
}

/// Generate a new random ethereum key.
/// A chainflip validator must have their own Ethereum private keys and be capable of submitting
/// transactions. We recommend importing the generated secret key into metamask for account
/// management.
/// returns the keypair and the derived ethereum address
pub fn generate_ethereum_key() -> (KeyPair, [u8; 20]) {
	use rand::SeedableRng;
	let mut rng = rand::rngs::StdRng::from_entropy();

	let secret_key = libsecp256k1::SecretKey::random(&mut rng);
	let public_key = libsecp256k1::PublicKey::from_secret_key(&secret_key);

	(
		KeyPair {
			secret_key: secret_key.serialize().to_vec(),
			public_key: public_key.serialize_compressed().to_vec(),
		},
		to_ethereum_address(public_key),
	)
}

#[test]
fn test_generate_signing_key_with_known_seed() {
	const SEED_PHRASE: &str =
		"essay awesome afraid movie wish save genius eyebrow tonight milk agree pretty alcohol three whale";

	let (generate_key, _, _) = generate_signing_key(Some(SEED_PHRASE)).unwrap();

	// Compare the generated secret key with a known secret key generated using the `chainflip-node
	// key generate` command
	assert_eq!(
		hex::encode(generate_key.secret_key),
		"afabf42a9a99910cdd64795ef05ed71acfa2238f5682d26ae62028df3cc59727"
	);
}
