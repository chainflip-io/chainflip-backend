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

use crate::RpcResult;

use cf_chains::{eth::Address as EthereumAddress, CcmChannelMetadataUnchecked};
use cf_rpc_types::{AccountId32, BlockUpdate, RefundParametersRpc, H256};
use jsonrpsee::proc_macros::rpc;
use state_chain_runtime::runtime_apis::OpenedDepositChannels;

pub use cf_primitives::DcaParameters;
pub use cf_rpc_types::broker::*;

#[rpc(server, client, namespace = "broker")]
pub trait BrokerRpcApi {
	#[method(name = "register_account", aliases = ["broker_registerAccount"])]
	async fn register_account(&self) -> RpcResult<String>;

	#[method(name = "request_swap_deposit_address", aliases = ["broker_requestSwapDepositAddress"])]
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
	) -> RpcResult<SwapDepositAddress>;

	#[method(name = "withdraw_fees", aliases = ["broker_withdrawFees"])]
	async fn withdraw_fees(
		&self,
		asset: Asset,
		destination_address: AddressString,
	) -> RpcResult<WithdrawFeesDetail>;

	#[method(name = "request_swap_parameter_encoding", aliases = ["broker_requestSwapParameterEncoding"])]
	async fn request_swap_parameter_encoding(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		extra_parameters: VaultSwapExtraParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<VaultSwapDetails<AddressString>>;

	#[method(name = "decode_vault_swap_parameter", aliases = ["broker_DecodeVaultSwapParameter"])]
	async fn decode_vault_swap_parameter(
		&self,
		vault_swap: VaultSwapDetails<AddressString>,
	) -> RpcResult<VaultSwapInputRpc>;

	#[method(name = "encode_cf_parameters", aliases = ["broker_EncodeCfParameters"])]
	async fn encode_cf_parameters(
		&self,
		source_asset: Asset,
		destination_asset: Asset,
		destination_address: AddressString,
		broker_commission: BasisPoints,
		refund_parameters: ChannelRefundParametersRpc,
		channel_metadata: Option<CcmChannelMetadataUnchecked>,
		boost_fee: Option<BasisPoints>,
		affiliate_fees: Option<Affiliates<AccountId32>>,
		dca_parameters: Option<DcaParameters>,
	) -> RpcResult<RpcBytes>;

	#[method(name = "mark_transaction_for_rejection", aliases = ["broker_MarkTransactionForRejection"])]
	async fn mark_transaction_for_rejection(&self, tx_id: TransactionInId) -> RpcResult<()>;

	#[method(name = "get_open_deposit_channels", aliases = ["broker_getOpenDepositChannels"])]
	async fn get_open_deposit_channels(
		&self,
		query: GetOpenDepositChannelsQuery,
	) -> RpcResult<ChainAccounts>;

	#[method(name = "all_open_deposit_channels", aliases = ["broker_allOpenDepositChannels"])]
	async fn all_open_deposit_channels(&self) -> RpcResult<Vec<OpenedDepositChannels>>;

	#[subscription(name = "subscribe_transaction_screening_events", item = BlockUpdate<TransactionScreeningEvents>)]
	async fn subscribe_transaction_screening_events(&self);

	#[method(name = "open_private_btc_channel", aliases = ["broker_openPrivateBtcChannel"])]
	async fn open_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "close_private_btc_channel", aliases = ["broker_closePrivateBtcChannel"])]
	async fn close_private_btc_channel(&self) -> RpcResult<ChannelId>;

	#[method(name = "register_affiliate", aliases = ["broker_registerAffiliate"])]
	async fn register_affiliate(
		&self,
		withdrawal_address: EthereumAddress,
	) -> RpcResult<AccountId32>;

	#[method(name = "get_affiliates", aliases = ["broker_getAffiliates"])]
	async fn get_affiliates(
		&self,
		affiliate: Option<AccountId32>,
	) -> RpcResult<Vec<(AccountId32, AffiliateDetails)>>;

	#[method(name = "affiliate_withdrawal_request", aliases = ["broker_affiliateWithdrawalRequest"])]
	async fn affiliate_withdrawal_request(
		&self,
		affiliate_account_id: AccountId32,
	) -> RpcResult<WithdrawFeesDetail>;

	#[method(name = "get_vault_addresses", aliases = ["broker_getVaultAddresses"])]
	async fn vault_addresses(&self) -> RpcResult<VaultAddresses>;

	#[method(name = "set_vault_swap_minimum_broker_fee", aliases =
	["broker_setVaultSwapMinimumBrokerFee"])]
	async fn set_vault_swap_minimum_broker_fee(
		&self,
		minimum_fee_bps: BasisPoints,
	) -> RpcResult<H256>;
}
