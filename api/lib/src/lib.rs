use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use cf_chains::eth::H256;
use cf_primitives::AccountRole;
use futures::StreamExt;
use pallet_cf_governance::ProposalId;
use pallet_cf_validator::MAX_LENGTH_FOR_VANITY_NAME;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{ed25519::Public as EdPublic, sr25519::Public as SrPublic, Bytes};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::opaque::SessionKeys;
use web3::types::H160;

pub mod primitives {
	pub use cf_primitives::*;
	pub use pallet_cf_governance::ProposalId;
	pub use state_chain_runtime::Hash;
}

pub use chainflip_node::chain_spec::use_chainflip_account_id_encoding;

use chainflip_engine::{
	eth::{rpc::EthDualRpcClient, EthBroadcaster},
	settings,
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcApi, BaseRpcClient, RawRpcApi},
		connect_to_state_chain,
		extrinsic_api::ExtrinsicApi,
		storage_api::StorageApi,
		StateChainClient,
	},
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
) -> Result<()> {
	println!("Querying the state chain for the block with hash {:x?}.", block_hash);

	let state_chain_rpc_client = BaseRpcClient::new(state_chain_settings).await?;

	match state_chain_rpc_client.block(block_hash).await? {
		Some(block) => {
			println!("{:#?}", block);
		},
		None => println!("Could not find block with block hash {:x?}", block_hash),
	}
	Ok(())
}

pub async fn request_claim(
	amount: f64,
	eth_address: [u8; 20],
	state_chain_settings: &settings::StateChain,
	eth_settings: &settings::Eth,
	should_register_claim: bool,
	logger: &slog::Logger,
) -> Result<()> {
	let (_, block_stream, state_chain_client) =
		connect_to_state_chain(state_chain_settings, false, logger).await?;

	// Are we in a current auction phase
	if state_chain_client.is_auction_phase().await? {
		bail!("We are currently in an auction phase. Please wait until the auction phase is over.");
	}

	let atomic_amount: u128 = (amount * 10_f64.powi(18)) as u128;

	println!(
		"Submitting claim with amount `{}` FLIP (`{}` Flipperinos) to ETH address `0x{}`.",
		amount,
		atomic_amount,
		hex::encode(eth_address)
	);

	if !confirm_submit() {
		return Ok(())
	}

	// Do the claim

	let tx_hash = state_chain_client
		.submit_signed_extrinsic(
			pallet_cf_staking::Call::claim { amount: atomic_amount.into(), address: eth_address },
			logger,
		)
		.await?;

	println!(
		"Your claim has transaction hash: `{:#x}`. Waiting for your request to be confirmed...",
		tx_hash
	);

	let mut block_stream = Box::new(block_stream);
	let block_stream = block_stream.as_mut();

	let events = state_chain_client.watch_submitted_extrinsic(tx_hash, block_stream).await?;

	for event in events {
		if let state_chain_runtime::Event::EthereumThresholdSigner(
			pallet_cf_threshold_signature::Event::ThresholdSignatureRequest { .. },
		) = event
		{
			println!("Your claim request is on chain.\nWaiting for signed claim data...");
			'outer: while let Some(result_header) = block_stream.next().await {
				let header = result_header.expect("Failed to get a valid block header");
				let block_hash = header.hash();
				let events = state_chain_client
					.storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(block_hash)
					.await?;
				for event_record in events {
					if let state_chain_runtime::Event::Staking(
						pallet_cf_staking::Event::ClaimSignatureIssued(validator_id, claim_cert),
					) = event_record.event
					{
						if validator_id == state_chain_client.account_id() {
							if should_register_claim {
								println!(
									"Your claim certificate is: {:?}",
									hex::encode(claim_cert.clone())
								);
								let chain_id = state_chain_client
									.storage_value::<pallet_cf_environment::EthereumChainId<
										state_chain_runtime::Runtime,
									>>(block_hash)
									.await
									.expect("Failed to fetch EthereumChainId from the State Chain");
								let stake_manager_address = state_chain_client
									.storage_value::<pallet_cf_environment::StakeManagerAddress<
										state_chain_runtime::Runtime,
									>>(block_hash)
									.await
									.expect("Failed to fetch StakeManagerAddress from State Chain");
								let tx_hash = register_claim(
									eth_settings,
									chain_id,
									stake_manager_address.into(),
									claim_cert,
									logger,
								)
								.await
								.expect("Failed to register claim on ETH");

								println!(
									"Submitted claim to Ethereum successfully with tx_hash: {:#x}",
									tx_hash
								);
							} else {
								println!("Your claim request has been successfully registered. Please proceed to the Staking UI to complete your claim.");
							}
							break 'outer
						}
					}
				}
			}
		}
	}
	Ok(())
}

/// Register the claim certificate on Ethereum
async fn register_claim(
	eth_settings: &settings::Eth,
	chain_id: u64,
	stake_manager_address: H160,
	claim_cert: Vec<u8>,
	logger: &slog::Logger,
) -> Result<H256> {
	println!(
		"Registering your claim on the Ethereum network, to StakeManager address: {:?}",
		stake_manager_address
	);

	let eth_broadcaster = EthBroadcaster::new(
		eth_settings,
		EthDualRpcClient::new(eth_settings, chain_id.into(), logger)
			.await
			.context("Could not create EthDualRpcClient")?,
		logger,
	)?;

	eth_broadcaster
		.send(
			eth_broadcaster
				.encode_and_sign_tx(cf_chains::eth::UnsignedTransaction {
					chain_id,
					contract: stake_manager_address,
					data: claim_cert,
					..Default::default()
				})
				.await?
				.0,
		)
		.await
}

pub async fn register_account_role(
	role: AccountRole,
	state_chain_settings: &settings::StateChain,
	logger: &slog::Logger,
) -> Result<()> {
	let (_, _, state_chain_client) =
		connect_to_state_chain(state_chain_settings, false, logger).await?;

	println!(
        "Submtting `register-account-role` with role: {:?}. This cannot be reversed for your account.",
        role
    );

	if !confirm_submit() {
		return Ok(())
	}

	let tx_hash = state_chain_client
		.submit_signed_extrinsic(
			pallet_cf_account_roles::Call::register_account_role { role },
			logger,
		)
		.await
		.expect("Could not set register account role for account");
	println!("Account role set at tx {:#x}.", tx_hash);
	Ok(())
}

pub async fn rotate_keys(
	state_chain_settings: &settings::StateChain,
	logger: &slog::Logger,
) -> Result<()> {
	let (_, _, state_chain_client) =
		connect_to_state_chain(state_chain_settings, false, logger).await?;
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
			pallet_cf_validator::Call::set_keys { keys: new_session_key, proof: [0; 1].to_vec() },
			logger,
		)
		.await
		.expect("Failed to submit set_keys extrinsic");

	println!("Session key rotated at tx {:#x}.", tx_hash);
	Ok(())
}

// Account must be the governance dictator in order for this to work.
pub async fn force_rotation(
	id: ProposalId,
	state_chain_settings: &settings::StateChain,
	logger: &slog::Logger,
) -> Result<()> {
	let (_, _, state_chain_client) =
		connect_to_state_chain(state_chain_settings, false, logger).await?;

	state_chain_client
		.submit_signed_extrinsic(
			pallet_cf_governance::Call::propose_governance_extrinsic {
				call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			},
			logger,
		)
		.await
		.expect("Should submit sudo governance proposal");

	println!("Submitting governance proposal for rotation.");

	state_chain_client
		.submit_signed_extrinsic(pallet_cf_governance::Call::approve { approved_id: id }, logger)
		.await
		.expect("Should submit approval, triggering execution of the forced rotation");

	println!("Approved governance proposal {}. Rotation should commence soon if you are the governance dictator", id);

	Ok(())
}

pub async fn retire_account(
	state_chain_settings: &settings::StateChain,
	logger: &slog::Logger,
) -> Result<()> {
	let (_, _, state_chain_client) =
		connect_to_state_chain(state_chain_settings, false, logger).await?;
	let tx_hash = state_chain_client
		.submit_signed_extrinsic(pallet_cf_staking::Call::retire_account {}, logger)
		.await
		.expect("Could not retire account");
	println!("Account retired at tx {:#x}.", tx_hash);
	Ok(())
}

pub async fn activate_account(
	state_chain_settings: &settings::StateChain,
	logger: &slog::Logger,
) -> Result<()> {
	let (latest_block_hash, _, state_chain_client) =
		connect_to_state_chain(state_chain_settings, false, logger).await?;

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
                .submit_signed_extrinsic(pallet_cf_staking::Call::activate_account {}, logger)
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
}

pub async fn set_vanity_name(
	name: String,
	state_chain_settings: &settings::StateChain,
	logger: &slog::Logger,
) -> Result<()> {
	if name.len() > MAX_LENGTH_FOR_VANITY_NAME {
		bail!("Name too long. Max length is {} characters.", MAX_LENGTH_FOR_VANITY_NAME,);
	}

	let (_, _, state_chain_client) =
		connect_to_state_chain(state_chain_settings, false, logger).await?;
	let tx_hash = state_chain_client
		.submit_signed_extrinsic(
			pallet_cf_validator::Call::set_vanity_name { name: name.as_bytes().to_vec() },
			logger,
		)
		.await
		.expect("Could not set vanity name for your account");
	println!("Vanity name set at tx {:#x}.", tx_hash);
	Ok(())
}

fn confirm_submit() -> bool {
	use std::{io, io::*};

	loop {
		print!("Do you wish to proceed? [y/n] > ");
		std::io::stdout().flush().unwrap();
		let mut input = String::new();
		io::stdin().read_line(&mut input).expect("Error: Failed to get user input");

		let input = input.trim();

		match input {
			"y" | "yes" | "1" | "true" | "ofc" => {
				println!("Submitting...");
				return true
			},
			"n" | "no" | "0" | "false" | "nah" => {
				println!("Ok, exiting...");
				return false
			},
			_ => continue,
		}
	}
}
