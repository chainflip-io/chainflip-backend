use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use cf_chains::eth::H256;
use cf_primitives::{AccountRole, Asset, ForeignChainAddress};
use futures::{FutureExt, Stream};
use pallet_cf_validator::MAX_LENGTH_FOR_VANITY_NAME;
use rand_legacy::FromEntropy;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{ed25519::Public as EdPublic, sr25519::Public as SrPublic, Bytes, Pair};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::opaque::SessionKeys;

pub mod primitives {
	pub use cf_primitives::*;
	pub use pallet_cf_governance::ProposalId;
	pub use state_chain_runtime::Hash;
	pub type ClaimAmount = pallet_cf_staking::ClaimAmount<FlipBalance>;
}

pub use chainflip_engine::settings;
pub use chainflip_node::chain_spec::use_chainflip_account_id_encoding;

use chainflip_engine::{
	logging::utils::new_discard_logger,
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcApi, BaseRpcClient, RawRpcApi},
		extrinsic_api::ExtrinsicApi,
		storage_api::StorageApi,
		StateChainClient,
	},
	task_scope::task_scope,
};

#[async_trait]
trait AuctionPhaseApi {
	async fn is_auction_phase(&self) -> Result<bool>;
}

#[async_trait]
impl<RawRpcClient: RawRpcApi + Send + Sync + 'static> AuctionPhaseApi
	for StateChainClient<BaseRpcClient<RawRpcClient>>
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
impl<RawRpcClient: RawRpcApi + Send + Sync + 'static> RotateSessionKeysApi
	for StateChainClient<BaseRpcClient<RawRpcClient>>
{
	async fn rotate_session_keys(&self) -> Result<Bytes> {
		let session_key_bytes: Bytes = self.base_rpc_client.raw_rpc_client.rotate_keys().await?;
		Ok(session_key_bytes)
	}
}

pub async fn request_block(
	block_hash: state_chain_runtime::Hash,
	state_chain_settings: &settings::StateChain,
) -> Result<state_chain_runtime::SignedBlock> {
	println!("Querying the state chain for the block with hash {block_hash:x?}.");

	let state_chain_rpc_client = BaseRpcClient::new(state_chain_settings).await?;

	state_chain_rpc_client
		.block(block_hash)
		.await?
		.ok_or_else(|| anyhow!("unknown block hash"))
}

pub type ClaimCertificate = Vec<u8>;

async fn submit_and_ensure_success<Call, BlockStream>(
	client: &StateChainClient,
	block_stream: &mut BlockStream,
	call: Call,
) -> Result<(H256, Vec<state_chain_runtime::RuntimeEvent>)>
where
	Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static,
	BlockStream: Stream<Item = state_chain_runtime::Header> + Unpin + Send + 'static,
{
	let logger = new_discard_logger();
	let tx_hash = client.submit_signed_extrinsic(call, &logger).await?;

	let events = client.watch_submitted_extrinsic(tx_hash, block_stream).await?;

	if let Some(_failure) = events.iter().find(|event| {
		matches!(
			event,
			state_chain_runtime::RuntimeEvent::System(frame_system::Event::ExtrinsicFailed { .. })
		)
	}) {
		Err(anyhow!("extrinsic execution failed"))
	} else {
		Ok((tx_hash, events))
	}
}

async fn connect_submit_and_get_events<Call>(
	state_chain_settings: &settings::StateChain,
	call: Call,
) -> Result<Vec<state_chain_runtime::RuntimeEvent>>
where
	Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static,
{
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			let (_, block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;

			let mut block_stream = Box::new(block_stream);

			let (_tx_hash, events) =
				submit_and_ensure_success(&state_chain_client, block_stream.as_mut(), call).await?;

			Ok(events)
		}
		.boxed()
	})
	.await
}

pub async fn request_claim(
	amount: primitives::ClaimAmount,
	eth_address: [u8; 20],
	state_chain_settings: &settings::StateChain,
) -> Result<H256> {
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			let (_, block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;

			// Are we in a current auction phase
			if state_chain_client.is_auction_phase().await? {
				bail!("We are currently in an auction phase. Please wait until the auction phase is over.");
			}

			let mut block_stream = Box::new(block_stream);
			let block_stream = block_stream.as_mut();

			let (tx_hash, _) = submit_and_ensure_success(
				&state_chain_client,
				block_stream,
				pallet_cf_staking::Call::claim { amount, address: eth_address },
			)
			.await
			.map_err(|_| anyhow!("invalid claim"))?;

			Ok(tx_hash)
		}
		.boxed()
	})
	.await
}

pub async fn register_account_role(
	role: AccountRole,
	state_chain_settings: &settings::StateChain,
) -> Result<()> {
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			let (_, _, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;

			let tx_hash = state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_account_roles::Call::register_account_role { role },
					&logger,
				)
				.await
				.expect("Could not set register account role for account");
			println!("Account role set at tx {tx_hash:#x}.");
			Ok(())
		}
		.boxed()
	})
	.await
}

pub async fn rotate_keys(state_chain_settings: &settings::StateChain) -> Result<H256> {
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			let (_, _, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;
			let seed = state_chain_client
				.rotate_session_keys()
				.await
				.expect("Could not rotate session keys.");

			let aura_key: [u8; 32] = seed[0..32].try_into().unwrap();
			let grandpa_key: [u8; 32] = seed[32..64].try_into().unwrap();

			let new_session_key = SessionKeys {
				aura: AuraId::from(SrPublic::from_raw(aura_key)),
				grandpa: GrandpaId::from(EdPublic::from_raw(grandpa_key)),
			};

			let tx_hash = state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_validator::Call::set_keys {
						keys: new_session_key,
						proof: [0; 1].to_vec(),
					},
					&logger,
				)
				.await
				.expect("Failed to submit set_keys extrinsic");

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
			let logger = new_discard_logger();
			let (_, _, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;

			println!("Submitting governance proposal for rotation.");
			state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_governance::Call::propose_governance_extrinsic {
						call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
					},
					&logger,
				)
				.await
				.expect("Should submit sudo governance proposal");

			println!("Rotation should begin soon :)");

			Ok(())
		}
		.boxed()
	})
	.await
}

pub async fn retire_account(state_chain_settings: &settings::StateChain) -> Result<()> {
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			let (_, _, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;
			let tx_hash = state_chain_client
				.submit_signed_extrinsic(pallet_cf_staking::Call::retire_account {}, &logger)
				.await
				.expect("Could not retire account");
			println!("Account retired at tx {tx_hash:#x}.");
			Ok(())
		}
		.boxed()
	})
	.await
}

pub async fn activate_account(state_chain_settings: &settings::StateChain) -> Result<()> {
	task_scope(|scope| async {
		let logger = new_discard_logger();
		let (latest_block_hash, _, state_chain_client) =
			StateChainClient::new(scope, state_chain_settings, AccountRole::None, false, &logger).await?;

		match state_chain_client
			.storage_map_entry::<pallet_cf_account_roles::AccountRoles<state_chain_runtime::Runtime>>(
				latest_block_hash,
				&state_chain_client.account_id(),
			)
			.await
			.expect("Failed to request AccountRole")
			.ok_or_else(|| anyhow!("Your account is not staked. You must first stake and then register your account role as Validator before activating your account."))?
		{
			AccountRole::Validator => {
				let tx_hash = state_chain_client
					.submit_signed_extrinsic(pallet_cf_staking::Call::activate_account {}, &logger)
					.await
					.expect("Could not activate account");
				println!("Account activated at tx {tx_hash:#x}.");
			}
			AccountRole::None => {
				println!("You have not yet registered an account role. If you wish to activate your account to gain a chance at becoming an authority on the Chainflip network
				you must first register your account as the Validator role. Please see the `register-account-role` command on this CLI.")
			}
			_ => {
				println!("You have already registered an account role for this account that is not the Validator role. You cannot activate your account for participation as an authority on the Chainflip network.")
			}
		}

		Ok(())
	}.boxed()).await
}

pub async fn set_vanity_name(
	name: String,
	state_chain_settings: &settings::StateChain,
) -> Result<()> {
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			if name.len() > MAX_LENGTH_FOR_VANITY_NAME {
				bail!("Name too long. Max length is {} characters.", MAX_LENGTH_FOR_VANITY_NAME,);
			}

			let (_, _, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;
			let tx_hash = state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_validator::Call::set_vanity_name { name: name.as_bytes().to_vec() },
					&logger,
				)
				.await
				.expect("Could not set vanity name for your account");
			println!("Vanity name set at tx {tx_hash:#x}.");
			Ok(())
		}
		.boxed()
	})
	.await
}

pub async fn register_swap_intent(
	state_chain_settings: &settings::StateChain,
	ingress_asset: Asset,
	egress_asset: Asset,
	egress_address: ForeignChainAddress,
	relayer_commission_bps: u16,
) -> Result<ForeignChainAddress> {
	let events = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_swapping::Call::register_swap_intent {
			ingress_asset,
			egress_asset,
			egress_address,
			relayer_commission_bps,
		},
	)
	.await?;

	if let Some(state_chain_runtime::RuntimeEvent::Swapping(
		pallet_cf_swapping::Event::NewSwapIntent { ingress_address, .. },
	)) = events.iter().find(|event| {
		matches!(
			event,
			state_chain_runtime::RuntimeEvent::Swapping(
				pallet_cf_swapping::Event::NewSwapIntent { .. }
			)
		)
	}) {
		Ok(*ingress_address)
	} else {
		panic!("NewSwapIntent must have been generated");
	}
}

pub async fn liquidity_deposit(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
) -> Result<ForeignChainAddress> {
	let events = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_lp::Call::request_deposit_address { asset },
	)
	.await?;

	if let Some(state_chain_runtime::RuntimeEvent::LiquidityProvider(
		pallet_cf_lp::Event::DepositAddressReady { ingress_address, intent_id: _ },
	)) = events.iter().find(|event| {
		matches!(
			event,
			state_chain_runtime::RuntimeEvent::LiquidityProvider(
				pallet_cf_lp::Event::DepositAddressReady { .. }
			)
		)
	}) {
		Ok(*ingress_address)
	} else {
		panic!("DepositAddressReady must have been generated");
	}
}

use zeroize::Zeroize;

#[derive(Debug, Zeroize)]
/// Public and Secret keys as bytes
pub struct KeyPair {
	pub secret_key: Vec<u8>,
	pub public_key: Vec<u8>,
}

/// Generate a new random node key.
/// This key is used for secure communication between Validators.
pub fn generate_node_key() -> KeyPair {
	use rand::SeedableRng;

	let mut rng = rand::rngs::StdRng::from_entropy();
	let keypair = ed25519_dalek::Keypair::generate(&mut rng);

	KeyPair {
		secret_key: keypair.secret.as_bytes().to_vec(),
		public_key: keypair.public.to_bytes().to_vec(),
	}
}

/// Generate a signing key (aka validator key) using the seed phrase.
/// If no seed phrase is provided, a new random seed phrase will be created.
/// Returns the key and the seed phrase used to create it.
/// This key is used to stake your node.
pub fn generate_signing_key(seed_phrase: Option<&str>) -> Result<(KeyPair, String)> {
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
			)
		})
		.map_err(|_| anyhow::Error::msg("Invalid seed phrase"))
}

/// Generate a new random ethereum key.
/// A chainflip validator must have their own Ethereum private keys and be capable of submitting
/// transactions. We recommend importing the generated secret key into metamask for account
/// management.
pub fn generate_ethereum_key() -> KeyPair {
	use secp256k1::Secp256k1;

	let mut rng = rand_legacy::rngs::StdRng::from_entropy();

	let (secret_key, public_key) = Secp256k1::new().generate_keypair(&mut rng);

	KeyPair { secret_key: secret_key[..].to_vec(), public_key: public_key.serialize().to_vec() }
}

#[test]
fn test_generate_signing_key_with_known_seed() {
	const SEED_PHRASE: &str =
		"essay awesome afraid movie wish save genius eyebrow tonight milk agree pretty alcohol three whale";

	let (generate_key, _) = generate_signing_key(Some(SEED_PHRASE)).unwrap();

	// Compare the generated secret key with a known secret key generated using the `chainflip-node
	// key generate` command
	assert_eq!(
		hex::encode(generate_key.secret_key),
		"afabf42a9a99910cdd64795ef05ed71acfa2238f5682d26ae62028df3cc59727"
	);
}
