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

use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
pub use cf_chains::address::AddressString;
use cf_chains::{evm::to_evm_address, CcmChannelMetadataUnchecked};
pub use cf_primitives::{AccountRole, Affiliates, Asset, BasisPoints, ChannelId, SemVer};
use cf_primitives::{DcaParameters, ForeignChain};
use cf_rpc_types::{RebalanceOutcome, RedemptionAmount, RedemptionOutcome, RefundParametersRpc};
use futures::{future::BoxFuture, FutureExt, TryFutureExt};
use pallet_cf_account_roles::MAX_LENGTH_FOR_VANITY_NAME;
use pallet_cf_governance::ExecutionMode;
use serde::Serialize;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
pub use sp_core::crypto::AccountId32;
use sp_core::{ed25519::Public as EdPublic, sr25519::Public as SrPublic, Bytes, Pair, H256};
pub use state_chain_runtime::chainflip::BlockUpdate;
use state_chain_runtime::{opaque::SessionKeys, RuntimeCall, RuntimeEvent};
use zeroize::Zeroize;
pub mod primitives {
	pub use cf_chains::{
		address::{EncodedAddress, ForeignChainAddress},
		CcmChannelMetadata, CcmDepositMetadata, Chain, ChainCrypto,
	};
	pub use cf_primitives::*;
	pub use pallet_cf_governance::ProposalId;
	pub use pallet_cf_swapping::AffiliateDetails;
	pub use state_chain_runtime::{self, BlockNumber, Hash};
}
pub use cf_chains::eth::Address as EthereumAddress;
use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, PolkadotInstance,
	SolanaInstance,
};
pub use cf_node_client::WaitForResult;

pub use chainflip_engine::{
	settings,
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcApi, RawRpcApi},
		chain_api::ChainApi,
		extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
		storage_api::StorageApi,
		BlockInfo,
	},
};

pub mod lp;
pub mod queries;

pub use chainflip_node::chain_spec::use_chainflip_account_id_encoding;

// TODO: consider exporting lp types under another alias to avoid shadowing this crate's lp module.
use cf_rpc_types::lp::LiquidityDepositChannelDetails;
pub use cf_rpc_types::{self as rpc_types, broker::*};
use cf_utilities::task_scope::Scope;
use chainflip_engine::state_chain_observer::client::{
	base_rpc_api::BaseRpcClient, extrinsic_api::signed::UntilInBlock, DefaultRpcClient,
	StateChainClient,
};

macro_rules! extract_event {
	($events:expr, $runtime_event_variant:path, $pallet_event_variant:path, $pattern:tt, $result:expr) => {
		if let Some($runtime_event_variant($pallet_event_variant $pattern)) = $events.iter().find(|event| {
			matches!(event, $runtime_event_variant($pallet_event_variant { .. }))
		}) {
			Ok($result)
		} else {
			bail!("No {}({}) event was found", stringify!($runtime_event_variant), stringify!($pallet_event_variant));
		}
	};
}

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
	pub async fn connect(
		scope: &Scope<'_, anyhow::Error>,
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

	pub fn deposit_monitor_api(&self) -> Arc<impl DepositMonitorApi> {
		self.state_chain_client.clone()
	}

	pub fn query_api(&self) -> queries::QueryApi {
		queries::QueryApi { state_chain_client: self.state_chain_client.clone() }
	}

	pub fn base_rpc_api(&self) -> Arc<impl BaseRpcApi + Send + Sync + 'static> {
		self.state_chain_client.base_rpc_client.clone()
	}

	pub fn raw_client(&self) -> &jsonrpsee::ws_client::WsClient {
		&self.state_chain_client.base_rpc_client.raw_rpc_client
	}
}

#[async_trait]
impl GovernanceApi for StateChainClient {}
#[async_trait]
impl BrokerApi for StateChainClient {
	fn raw_rpc_client(&self) -> &jsonrpsee::ws_client::WsClient {
		&self.base_rpc_client.raw_rpc_client
	}

	fn base_rpc_client(&self) -> Arc<DefaultRpcClient> {
		self.base_rpc_client.clone()
	}
}
#[async_trait]
impl OperatorApi for StateChainClient {}
#[async_trait]
impl ValidatorApi for StateChainClient {}
#[async_trait]
impl DepositMonitorApi for StateChainClient {}

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
	async fn accept_operator(&self, operator: AccountId32) -> Result<Vec<RuntimeEvent>> {
		let extrinsic_data = self
			.submit_signed_extrinsic_with_dry_run(pallet_cf_validator::Call::accept_operator {
				operator,
			})
			.await?
			.until_finalized()
			.await?;
		Ok(extrinsic_data.events)
	}
	async fn remove_operator(&self) -> Result<Vec<RuntimeEvent>> {
		let validator_account = self.account_id();
		let extrinsic_data = self
			.submit_signed_extrinsic_with_dry_run(pallet_cf_validator::Call::remove_validator {
				validator: validator_account,
			})
			.await?
			.until_finalized()
			.await?;
		Ok(extrinsic_data.events)
	}
}

#[async_trait]
pub trait OperatorApi: SignedExtrinsicApi + RotateSessionKeysApi + AuctionPhaseApi {
	async fn request_redemption(
		&self,
		amount: RedemptionAmount,
		address: EthereumAddress,
		executor: Option<EthereumAddress>,
	) -> Result<RedemptionOutcome> {
		let extrinsic_data = self
			.submit_signed_extrinsic_with_dry_run(pallet_cf_funding::Call::redeem {
				amount,
				address,
				executor,
			})
			.await?
			.until_finalized()
			.await?;

		extract_event!(
			extrinsic_data.events,
			state_chain_runtime::RuntimeEvent::Funding,
			pallet_cf_funding::Event::RedemptionRequested,
			{ account_id, amount, .. },
			RedemptionOutcome {
				source_account_id: account_id.clone(),
				redeem_address: address,
				amount: *amount,
				tx_hash: extrinsic_data.tx_hash
			}
		)
	}

	async fn bind_redeem_address(&self, address: EthereumAddress) -> Result<H256> {
		Ok(self
			.submit_signed_extrinsic(pallet_cf_funding::Call::bind_redeem_address { address })
			.await
			.until_in_block()
			.await?
			.tx_hash)
	}

	async fn bind_executor_address(&self, executor_address: EthereumAddress) -> Result<H256> {
		Ok(self
			.submit_signed_extrinsic(pallet_cf_funding::Call::bind_executor_address {
				executor_address,
			})
			.await
			.until_finalized()
			.await?
			.tx_hash)
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
			AccountRole::Operator => bail!("Operator registration not supported via CLI."),
		};

		Ok(self
			.submit_signed_extrinsic_with_dry_run(call)
			.await?
			.until_in_block()
			.await?
			.tx_hash)
	}

	async fn rotate_session_keys(&self) -> Result<H256> {
		let raw_keys = RotateSessionKeysApi::rotate_session_keys(self).await?;

		let aura_key: [u8; 32] = raw_keys[0..32].try_into().unwrap();
		let grandpa_key: [u8; 32] = raw_keys[32..64].try_into().unwrap();

		Ok(self
			.submit_signed_extrinsic(pallet_cf_validator::Call::set_keys {
				keys: SessionKeys {
					aura: AuraId::from(SrPublic::from_raw(aura_key)),
					grandpa: GrandpaId::from(EdPublic::from_raw(grandpa_key)),
				},
				proof: [0; 1].to_vec(),
			})
			.await
			.until_in_block()
			.await?
			.tx_hash)
	}

	async fn set_vanity_name(&self, name: String) -> Result<()> {
		let tx_hash = self
			.submit_signed_extrinsic(pallet_cf_account_roles::Call::set_vanity_name {
				name: name.into_bytes().try_into().or_else(|_| {
					bail!("Name too long. Max length is {} characters.", MAX_LENGTH_FOR_VANITY_NAME,)
				})?,
			})
			.await
			.until_in_block()
			.await?
			.tx_hash;
		println!("Vanity name set at tx {tx_hash:#x}.");
		Ok(())
	}

	async fn request_rebalance(
		&self,
		amount: RedemptionAmount,
		redemption_address: Option<EthereumAddress>,
		recipient_account_id: AccountId32,
	) -> Result<RebalanceOutcome> {
		let extrinsic_data = self
			.submit_signed_extrinsic_with_dry_run(pallet_cf_funding::Call::rebalance {
				amount,
				recipient_account_id,
				redemption_address,
			})
			.await?
			.until_finalized()
			.await?;

		extract_event!(
			extrinsic_data.events,
			state_chain_runtime::RuntimeEvent::Funding,
			pallet_cf_funding::Event::Rebalance,
			{ source_account_id, recipient_account_id, amount, },
			RebalanceOutcome {
				source_account_id: source_account_id.clone(),
				recipient_account_id: recipient_account_id.clone(),
				amount: *amount
			}
		)
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

#[async_trait]
pub trait BrokerApi: SignedExtrinsicApi + StorageApi + Sized + Send + Sync + 'static {
	fn raw_rpc_client(&self) -> &jsonrpsee::ws_client::WsClient;

	fn base_rpc_client(&self) -> Arc<DefaultRpcClient>;

	async fn request_swap_deposit_address(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		refund_parameters: RefundParametersRpc,
		dca_parameters: Option<DcaParameters>,
	) -> Result<SwapDepositAddress> {
		let submit_signed_extrinsic_fut = self
			.submit_signed_extrinsic_with_dry_run(
				pallet_cf_swapping::Call::request_swap_deposit_address_with_affiliates {
					source_asset,
					destination_asset,
					destination_address: destination_address
						.try_parse_to_encoded_address(destination_asset.into())?,
					broker_commission,
					channel_metadata,
					boost_fee: boost_fee.unwrap_or_default(),
					affiliate_fees: affiliate_fees.unwrap_or_default(),
					refund_parameters: refund_parameters.try_map_address(|addr| {
						addr.try_parse_to_encoded_address(source_asset.into())
					})?,
					dca_parameters,
				},
			)
			.and_then(|(_, (block_fut, finalized_fut))| async move {
				let extrinsic_data = block_fut.until_in_block().await?;
				Ok((
					extract_swap_deposit_address(extrinsic_data.events, extrinsic_data.header)?,
					finalized_fut,
				))
			})
			.boxed();

		// Get the pre-allocated channels from the previous finalized block
		let preallocated_channels_fut = fetch_preallocated_channels(
			self.base_rpc_client(),
			self.account_id(),
			source_asset.into(),
		);

		let ((swap_deposit_address, finalized_fut), preallocated_channels) =
			futures::try_join!(submit_signed_extrinsic_fut, preallocated_channels_fut)?;

		// If the extracted deposit channel was pre-allocated to this broker
		// in the previous finalized block, we can return it immediately.
		if preallocated_channels.contains(&swap_deposit_address.channel_id) {
			return Ok(swap_deposit_address);
		};

		// Worst case, we need to wait for the transaction to be finalized.
		let extrinsic_data = finalized_fut.until_finalized().await?;
		extract_swap_deposit_address(extrinsic_data.events, extrinsic_data.header)
	}
	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> Result<WithdrawFeesDetail> {
		let extrinsic_data = self
			.submit_signed_extrinsic(RuntimeCall::from(pallet_cf_swapping::Call::withdraw {
				asset,
				destination_address: destination_address
					.try_parse_to_encoded_address(asset.into())
					.map_err(anyhow::Error::msg)?,
			}))
			.await
			.until_in_block()
			.await?;

		extract_event!(
			extrinsic_data.events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::WithdrawalRequested,
			{
				egress_amount,
				egress_fee,
				destination_address,
				egress_id,
				..
			},
			WithdrawFeesDetail {
				tx_hash: extrinsic_data.tx_hash,
				egress_id: *egress_id,
				egress_amount: (*egress_amount).into(),
				egress_fee: (*egress_fee).into(),
				destination_address: AddressString::from_encoded_address(destination_address),
			}
		)
	}
	async fn register_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_swapping::Call::register_as_broker {})
			.await
	}
	async fn deregister_account(&self) -> Result<H256> {
		self.simple_submission_with_dry_run(pallet_cf_swapping::Call::deregister_as_broker {})
			.await
	}

	async fn open_private_btc_channel(&self) -> Result<ChannelId> {
		let events = self
			.submit_signed_extrinsic_with_dry_run(RuntimeCall::from(
				pallet_cf_swapping::Call::open_private_btc_channel {},
			))
			.await?
			.until_in_block()
			.await?
			.events;

		extract_event!(
			&events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::PrivateBrokerChannelOpened,
			{ channel_id, .. },
			*channel_id
		)
	}

	async fn close_private_btc_channel(&self) -> Result<ChannelId> {
		let events = self
			.submit_signed_extrinsic_with_dry_run(RuntimeCall::from(
				pallet_cf_swapping::Call::close_private_btc_channel {},
			))
			.await?
			.until_in_block()
			.await?
			.events;

		extract_event!(
			&events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::PrivateBrokerChannelClosed,
			{ channel_id, .. },
			*channel_id
		)
	}

	async fn register_affiliate(&self, withdrawal_address: EthereumAddress) -> Result<AccountId32> {
		let events = self
			.submit_signed_extrinsic_with_dry_run(pallet_cf_swapping::Call::register_affiliate {
				withdrawal_address,
			})
			.await?
			.until_in_block()
			.await?
			.events;

		extract_event!(
			&events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::AffiliateRegistration,
			{ affiliate_id, .. },
			affiliate_id.clone()
		)
	}

	async fn affiliate_withdrawal_request(
		&self,
		affiliate_account_id: AccountId32,
	) -> Result<WithdrawFeesDetail> {
		let extrinsic_data = self
			.submit_signed_extrinsic_with_dry_run(
				pallet_cf_swapping::Call::affiliate_withdrawal_request { affiliate_account_id },
			)
			.await?
			.until_in_block()
			.await?;

		extract_event!(
			extrinsic_data.events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::WithdrawalRequested,
			{
				egress_amount,
				egress_fee,
				destination_address,
				egress_id,
				..
			},
			WithdrawFeesDetail {
				tx_hash: extrinsic_data.tx_hash,
				egress_id: *egress_id,
				egress_amount: (*egress_amount).into(),
				egress_fee: (*egress_fee).into(),
				destination_address: AddressString::from_encoded_address(destination_address),
			}
		)
	}

	async fn set_vault_swap_minimum_broker_fee(
		&self,
		minimum_fee_bps: BasisPoints,
	) -> Result<H256> {
		let extrinsic_data = self
			.submit_signed_extrinsic_with_dry_run(
				pallet_cf_swapping::Call::set_vault_swap_minimum_broker_fee { minimum_fee_bps },
			)
			.await?
			.until_in_block()
			.await?;

		extract_event!(
			extrinsic_data.events,
			state_chain_runtime::RuntimeEvent::Swapping,
			pallet_cf_swapping::Event::VaultSwapMinimumBrokerFeeSet,
			{ .. },
			extrinsic_data.tx_hash
		)
	}
}

#[async_trait]
pub trait SimpleSubmissionApi: SignedExtrinsicApi {
	async fn simple_submission_with_dry_run<C>(&self, call: C) -> Result<H256>
	where
		C: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static,
	{
		Ok(self
			.submit_signed_extrinsic_with_dry_run(call)
			.await?
			.until_in_block()
			.await?
			.tx_hash)
	}
}

#[async_trait]
impl<T: SignedExtrinsicApi + Sized + Send + Sync + 'static> SimpleSubmissionApi for T {}

#[async_trait]
pub trait DepositMonitorApi:
	SignedExtrinsicApi + StorageApi + Sized + Send + Sync + 'static
{
	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> Result<H256> {
		match tx_id {
			TransactionInId::Bitcoin(tx_id) =>
				self.simple_submission_with_dry_run(
					state_chain_runtime::RuntimeCall::BitcoinIngressEgress(
						pallet_cf_ingress_egress::Call::mark_transaction_for_rejection { tx_id },
					),
				)
				.await,
			TransactionInId::Ethereum(tx_id) =>
				self.simple_submission_with_dry_run(
					state_chain_runtime::RuntimeCall::EthereumIngressEgress(
						pallet_cf_ingress_egress::Call::mark_transaction_for_rejection { tx_id },
					),
				)
				.await,
			TransactionInId::Arbitrum(tx_id) =>
				self.simple_submission_with_dry_run(
					state_chain_runtime::RuntimeCall::ArbitrumIngressEgress(
						pallet_cf_ingress_egress::Call::mark_transaction_for_rejection { tx_id },
					),
				)
				.await,
		}
	}
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

fn fetch_preallocated_channels(
	rpc_client: Arc<DefaultRpcClient>,
	account_id: AccountId32,
	chain: ForeignChain,
) -> BoxFuture<'static, Result<Vec<ChannelId>>> {
	async fn preallocated_channels_for_chain<T: pallet_cf_ingress_egress::Config<I>, I: 'static>(
		client: Arc<DefaultRpcClient>,
		account_id: T::AccountId,
	) -> Result<Vec<ChannelId>> {
		Ok(client
			.storage_map_entry::<pallet_cf_ingress_egress::PreallocatedChannels<T, I>>(
				client.latest_finalized_block_hash().await?,
				&account_id,
			)
			.await?
			.iter()
			.map(|channel| channel.channel_id)
			.collect())
	}

	match chain {
		ForeignChain::Bitcoin => Box::pin(preallocated_channels_for_chain::<
			state_chain_runtime::Runtime,
			BitcoinInstance,
		>(rpc_client, account_id)),
		ForeignChain::Ethereum => Box::pin(preallocated_channels_for_chain::<
			state_chain_runtime::Runtime,
			EthereumInstance,
		>(rpc_client, account_id)),
		ForeignChain::Polkadot => Box::pin(preallocated_channels_for_chain::<
			state_chain_runtime::Runtime,
			PolkadotInstance,
		>(rpc_client, account_id)),
		ForeignChain::Arbitrum => Box::pin(preallocated_channels_for_chain::<
			state_chain_runtime::Runtime,
			ArbitrumInstance,
		>(rpc_client, account_id)),
		ForeignChain::Solana => Box::pin(preallocated_channels_for_chain::<
			state_chain_runtime::Runtime,
			SolanaInstance,
		>(rpc_client, account_id)),
		ForeignChain::Assethub => Box::pin(preallocated_channels_for_chain::<
			state_chain_runtime::Runtime,
			AssethubInstance,
		>(rpc_client, account_id)),
	}
}

fn extract_swap_deposit_address(
	events: Vec<RuntimeEvent>,
	header: state_chain_runtime::Header,
) -> Result<SwapDepositAddress> {
	extract_event!(
		events,
		state_chain_runtime::RuntimeEvent::Swapping,
		pallet_cf_swapping::Event::SwapDepositAddressReady,
		{
			deposit_address,
			channel_id,
			source_chain_expiry_block,
			channel_opening_fee,
			refund_parameters,
			..
		},
		SwapDepositAddress {
			address: AddressString::from_encoded_address(deposit_address),
			issued_block: header.number,
			channel_id: *channel_id,
			source_chain_expiry_block: (*source_chain_expiry_block).into(),
			channel_opening_fee: (*channel_opening_fee).into(),
			refund_parameters: refund_parameters.clone()
				.map_address(|refund_address| {
					AddressString::from_encoded_address(&refund_address)
				}),
		}
	)
}

fn extract_liquidity_deposit_channel_details(
	events: Vec<RuntimeEvent>,
) -> Result<(ChannelId, LiquidityDepositChannelDetails)> {
	events
		.into_iter()
		.find_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityProvider(
				pallet_cf_lp::Event::LiquidityDepositAddressReady {
					channel_id,
					deposit_address,
					deposit_chain_expiry_block,
					..
				},
			) => Some((
				channel_id,
				LiquidityDepositChannelDetails {
					deposit_address: AddressString::from_encoded_address(deposit_address),
					deposit_chain_expiry_block,
				},
			)),
			_ => None,
		})
		.ok_or_else(|| anyhow!("No LiquidityDepositAddressReady event was found"))
}

#[cfg(test)]
mod tests {
	use super::*;

	mod key_generation {

		use super::*;
		use cf_chains::{address::clean_foreign_chain_address, ForeignChain};
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

		#[test]
		fn test_sol_address_decoding() {
			assert_eq!(
				clean_foreign_chain_address(
					ForeignChain::Solana,
					"HGgUaHpsmZpB3pcYt8PE89imca6BQBRqYtbVQQqsso3o"
				)
				.unwrap(),
				clean_foreign_chain_address(
					ForeignChain::Solana,
					"0xf1bf5683e0bfb6fffacb2d8d3641faa0008b65cc296c26ec80aee5a71ddf294a"
				)
				.unwrap(),
			);
		}
	}
}
