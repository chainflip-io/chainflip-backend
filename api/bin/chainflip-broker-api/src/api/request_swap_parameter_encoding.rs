use crate::api::request_swap_deposit_address;
use cf_utilities::rpc::NumberOrHex;
use chainflip_api::{
	primitives::{
		state_chain_runtime::runtime_apis::VaultSwapDetails, BlockNumber, RefundParameters,
		VaultSwapExtraParameters,
	},
	AddressString, BaseRpcApi, ChainflipApi, CustomApiClient, EvmVaultSwapExtraParameters,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ApiWrapper, MockApi};

pub struct Endpoint;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VaultSwapParametersRequest<A> {
	#[serde(flatten)]
	pub inner: request_swap_deposit_address::Request<A>,
	#[serde(flatten)]
	pub extra: VaultSwapExtraParameters<A, cf_utilities::rpc::NumberOrHex>,
}

impl<A: JsonSchema> JsonSchema for VaultSwapParametersRequest<A> {
	fn schema_name() -> std::borrow::Cow<'static, str> {
		format!("VaultSwapParametersRequest<{}>", A::schema_name()).into()
	}

	fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
		let mut schema = VaultSwapParametersRequestShape::<
			A,
			request_swap_deposit_address::Request<A>,
		>::json_schema(generator);
		let map = schema.as_object_mut().unwrap();
		map["description"] =
			"Required parameters for requesting encoded Vault Swap parameters.".into();
		schema
	}
}

// Note this is required because json-schema can't infer the correct schema for the actual request
// type. Nested `flatten` works, but a flattened (untagged) enum next to a flattened struct does
// not. We might want to consider replacing VaultSwapExtraParameters with this.
/// A request
#[allow(dead_code)]
#[derive(JsonSchema)]
#[schemars(tag = "chain")]
enum VaultSwapParametersRequestShape<A, Common> {
	/// Request encoded Vault Swap parameters for Bitcoin.
	Bitcoin {
		#[schemars(flatten)]
		common: Common,
		min_output_amount: NumberOrHex,
		retry_duration: BlockNumber,
	},
	/// Request encoded Vault Swap parameters for Ethereum.
	Ethereum {
		#[schemars(flatten)]
		common: Common,
		extra: EvmVaultSwapExtraParameters<A, NumberOrHex>,
	},
	/// Request encoded Vault Swap parameters for Arbitrum.
	Arbitrum {
		#[schemars(flatten)]
		common: Common,
		extra: EvmVaultSwapExtraParameters<A, NumberOrHex>,
	},
	/// Request encoded Vault Swap parameters for Solana.
	Solana {
		#[schemars(flatten)]
		common: Common,
		from: A,
		event_data_account: A,
		input_amount: NumberOrHex,
		refund_parameters: RefundParameters<A>,
		from_token_account: Option<A>,
	},
}

impl api_json_schema::Endpoint for Endpoint {
	type Request = VaultSwapParametersRequest<AddressString>;
	type Response = VaultSwapDetails<AddressString>;
	type Error = anyhow::Error;
}

impl<T: ChainflipApi> api_json_schema::Responder<Endpoint> for ApiWrapper<T> {
	async fn respond(
		&self,
		VaultSwapParametersRequest {
			inner:
				request_swap_deposit_address::Request {
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					channel_metadata,
					boost_fee,
					affiliate_fees,
					refund_parameters: _,
					dca_parameters,
				},
			extra,
		}: api_json_schema::EndpointRequest<Endpoint>,
	) -> api_json_schema::EndpointResult<Endpoint> {
		Ok(self
			.base_rpc_api()
			.raw_rpc_client()
			.cf_get_vault_swap_details(
				self.account_id(),
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				extra,
				channel_metadata,
				boost_fee,
				affiliate_fees,
				dca_parameters,
				None,
			)
			.await?)
	}
}

impl api_json_schema::Responder<Endpoint> for MockApi {
	async fn respond(
		&self,
		// TODO: use the request payload to return examples for the correct chain
		_request: <Endpoint as api_json_schema::Endpoint>::Request,
	) -> api_json_schema::EndpointResult<Endpoint> {
		Ok(VaultSwapDetails::Bitcoin {
			// Generated from the test code for UtxoEncodedData.
			nulldata_payload: hex::decode(
				"000409090909090909090909090909090909090909090909090909090909090909090500ffffffffffffffffffffffffffffffffffff0200050a0806070809").unwrap(),
			deposit_address: "bc1pw75mqye4q9t0m649vtk0a9clsrf2fagq3mr6agekfzsx0lulfyxsvxqadt"
				.parse()
				.unwrap(),
			expires_at: 1687520000,
		})
	}
}
