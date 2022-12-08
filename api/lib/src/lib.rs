use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use cf_chains::eth::H256;
use cf_primitives::AccountRole;
use futures::{FutureExt, Stream, StreamExt};
use pallet_cf_governance::ProposalId;
use pallet_cf_validator::MAX_LENGTH_FOR_VANITY_NAME;
use rand_legacy::FromEntropy;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{ed25519::Public as EdPublic, sr25519::Public as SrPublic, Bytes, Pair};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::opaque::SessionKeys;

use custom_rpc::CustomApiClient;

#[cfg(feature = "ibiza")]
use cf_primitives::{Asset, ForeignChainAddress};

pub mod primitives {
	pub use cf_primitives::*;
	pub use pallet_cf_governance::ProposalId;
	pub use state_chain_runtime::Hash;
	pub type ClaimAmount = pallet_cf_staking::ClaimAmount<FlipBalance>;
}

pub use chainflip_node::chain_spec::use_chainflip_account_id_encoding;

use chainflip_engine::{
	eth::{rpc::EthDualRpcClient, EthBroadcaster},
	logging::utils::new_discard_logger,
	settings,
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
	println!("Querying the state chain for the block with hash {:x?}.", block_hash);

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
) -> Result<(H256, Vec<state_chain_runtime::Event>)>
where
	Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync + 'static,
	BlockStream: Stream<Item = state_chain_runtime::Header> + Unpin + Send + 'static,
{
	let logger = new_discard_logger();
	let tx_hash = client.submit_signed_extrinsic(call, &logger).await?;

	let events = client.watch_submitted_extrinsic(tx_hash, block_stream).await?;

	if let Some(_failure) = events.iter().find(|event| {
		matches!(
			event,
			state_chain_runtime::Event::System(frame_system::Event::ExtrinsicFailed { .. })
		)
	}) {
		Err(anyhow!("extrinsic execution failed"))
	} else {
		Ok((tx_hash, events))
	}
}

#[cfg(feature = "ibiza")]
async fn connect_submit_and_get_events<Call>(
	state_chain_settings: &settings::StateChain,
	call: Call,
) -> Result<Vec<state_chain_runtime::Event>>
where
	Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync + 'static,
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

/// Get claim certificate by polling the State Chain.
/// Returns `None` if no certificate is detected after polling
/// for `blocks_to_poll_limit` blocks.
pub async fn poll_for_claim_certificate(
	state_chain_settings: &settings::StateChain,
	blocks_to_poll_limit: usize,
) -> Result<Option<ClaimCertificate>> {
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			let (_block_hash, mut block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;

			let account_id = state_chain_client.account_id();

			let mut blocks_polled = 0;

			while let Some(_result_header) = block_stream.next().await {
				if let Some(certificate) = state_chain_client
					.base_rpc_client
					.raw_rpc_client
					.cf_get_claim_certificate(account_id.clone(), None)
					.await?
				{
					return Ok(Some(certificate))
				}

				blocks_polled += 1;
				if blocks_polled >= blocks_to_poll_limit {
					return Ok(None)
				}
			}

			Err(anyhow!("Block stream unexpectedly ended"))
		}
		.boxed()
	})
	.await
}

/// Register the claim certificate on Ethereum
pub async fn register_claim(
	eth_settings: &settings::Eth,
	state_chain_settings: &settings::StateChain,
	claim_cert: ClaimCertificate,
) -> Result<H256> {
	task_scope(|scope| {
		async {
			let logger = new_discard_logger();
			let (_, _block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::None,
				false,
				&logger,
			)
			.await?;

			let block_hash =
				state_chain_client.base_rpc_client.latest_finalized_block_hash().await?;

			let chain_id = state_chain_client
				.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
					block_hash,
				)
				.await
				.expect("Failed to fetch EthereumChainId from the State Chain");
			let stake_manager_address = state_chain_client
				.storage_value::<pallet_cf_environment::EthereumStakeManagerAddress<state_chain_runtime::Runtime>>(
					block_hash,
				)
				.await
				.expect("Failed to fetch EthereumStakeManagerAddress from State Chain");

			println!(
				"Registering your claim on the Ethereum network, to StakeManager address: {:?}",
				stake_manager_address
			);

			let eth_broadcaster = EthBroadcaster::new(
				eth_settings,
				EthDualRpcClient::new(eth_settings, chain_id.into(), &logger)
					.await
					.context("Could not create EthDualRpcClient")?,
				&logger,
			)?;

			eth_broadcaster
				.send(
					eth_broadcaster
						.encode_and_sign_tx(cf_chains::eth::Transaction {
							chain_id,
							contract: stake_manager_address.into(),
							data: claim_cert,
							..Default::default()
						})
						.await?
						.0,
				)
				.await
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
			println!("Account role set at tx {:#x}.", tx_hash);
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
pub async fn force_rotation(
	id: ProposalId,
	state_chain_settings: &settings::StateChain,
) -> Result<()> {
	task_scope(|scope| async {
		let logger = new_discard_logger();
		let (_, _, state_chain_client) =
			StateChainClient::new(scope, state_chain_settings, AccountRole::None, false, &logger).await?;

		state_chain_client
			.submit_signed_extrinsic(
				pallet_cf_governance::Call::propose_governance_extrinsic {
					call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
				},
				&logger,
			)
			.await
			.expect("Should submit sudo governance proposal");

		println!("Submitting governance proposal for rotation.");

		state_chain_client
			.submit_signed_extrinsic(pallet_cf_governance::Call::approve { approved_id: id }, &logger)
			.await
			.expect("Should submit approval, triggering execution of the forced rotation");

		println!("Approved governance proposal {}. Rotation should commence soon if you are the governance dictator", id);

		Ok(())
	}.boxed()).await
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
			println!("Account retired at tx {:#x}.", tx_hash);
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
				println!("Account activated at tx {:#x}.", tx_hash);
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
			println!("Vanity name set at tx {:#x}.", tx_hash);
			Ok(())
		}
		.boxed()
	})
	.await
}

#[cfg(feature = "ibiza")]
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

	if let Some(state_chain_runtime::Event::Swapping(pallet_cf_swapping::Event::NewSwapIntent {
		ingress_address,
		..
	})) = events.iter().find(|event| {
		matches!(
			event,
			state_chain_runtime::Event::Swapping(pallet_cf_swapping::Event::NewSwapIntent { .. })
		)
	}) {
		Ok(*ingress_address)
	} else {
		panic!("NewSwapIntent must have been generated");
	}
}

#[cfg(feature = "ibiza")]
pub async fn liquidity_deposit(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
) -> Result<ForeignChainAddress> {
	let events = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_lp::Call::request_deposit_address { asset },
	)
	.await?;

	if let Some(state_chain_runtime::Event::LiquidityProvider(
		pallet_cf_lp::Event::DepositAddressReady { ingress_address, intent_id: _ },
	)) = events.iter().find(|event| {
		matches!(
			event,
			state_chain_runtime::Event::LiquidityProvider(
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
/// Hex encoded keys without the `0x` prefix.
pub struct KeyPairHex {
	pub secret_key: Vec<u8>,
	pub public_key: String,
}

/// Generate a new random node key.
/// This key is used for secure communication between Validators.
pub fn generate_node_key() -> KeyPairHex {
	use rand::SeedableRng;

	let mut rng = rand::rngs::StdRng::from_entropy();
	let keypair = ed25519_dalek::Keypair::generate(&mut rng);

	KeyPairHex {
		secret_key: hex::encode(keypair.secret.as_bytes()).into_bytes().to_vec(),
		public_key: hex::encode(keypair.public.to_bytes()),
	}
}

/// Generate a signing key (aka validator key) using the seed phrase.
/// If no seed phrase is provided, a new random seed phrase will be created.
/// Returns the key and the seed phrase used to create it.
/// This key is used to stake your node.
pub fn generate_signing_key(seed_phrase: Option<&str>) -> Result<(KeyPairHex, String)> {
	use bip39::{Language, Mnemonic, MnemonicType};

	// Get a new random seed phrase if one was not provided
	let mnemonic = Mnemonic::new(MnemonicType::Words12, Language::English);
	let seed_phrase = seed_phrase.unwrap_or_else(|| mnemonic.phrase());

	if let Ok((pair, seed)) = sp_core::Pair::from_phrase(seed_phrase, None) {
		let pair: sp_core::sr25519::Pair = pair;

		Ok((
			KeyPairHex { secret_key: seed.to_vec(), public_key: hex::encode(pair.public()) },
			seed_phrase.to_string(),
		))
	} else {
		anyhow::bail!("Invalid seed phrase")
	}
}

/// Generate a new random ethereum key.
/// A chainflip validator must have their own Ethereum private keys and be capable of submitting
/// transactions. We recommend importing the generated secret key into metamask for account
/// management.
pub fn generate_ethereum_key() -> KeyPairHex {
	use secp256k1::Secp256k1;

	let mut rng = rand_legacy::rngs::StdRng::from_entropy();

	let (secret_key, public_key) = Secp256k1::new().generate_keypair(&mut rng);

	KeyPairHex {
		secret_key: secret_key.to_string().as_bytes().to_vec(),
		public_key: hex::encode(public_key.serialize()),
	}
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
