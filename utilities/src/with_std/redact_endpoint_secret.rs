use anyhow::Context;
use regex::Regex;
use url::Url;

const MAX_SECRET_CHARACTERS_REVEALED: usize = 3;
const SCHEMA_PADDING_LEN: usize = 3;

/// Partially redacts the secret in the url of the node endpoint.
///  eg: `wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/` ->
/// `wss://cdc****.rinkeby.ws.rivet.cloud/`
#[allow(unused)]
pub fn redact_secret_eth_node_endpoint(endpoint: &str) -> Result<String, anyhow::Error> {
	let re = Regex::new(r"[0-9a-fA-F]{32}").unwrap();
	if re.is_match(endpoint) {
		// A 32 character hex string was found, redact it
		let mut endpoint_redacted = endpoint.to_string();
		for capture in re.captures_iter(endpoint) {
			endpoint_redacted = endpoint_redacted.replace(
				&capture[0],
				&format!(
					"{}****",
					&capture[0].split_at(capture[0].len().min(MAX_SECRET_CHARACTERS_REVEALED)).0
				),
			);
		}
		Ok(endpoint_redacted)
	} else {
		// No secret was found, so just redact almost all of the url
		let url = Url::parse(endpoint).context("Failed to parse node endpoint into a URL")?;
		Ok(format!(
			"{}****",
			endpoint
				.split_at(usize::min(
					url.scheme().len() + SCHEMA_PADDING_LEN + MAX_SECRET_CHARACTERS_REVEALED,
					endpoint.len()
				))
				.0
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_secret_web_addresses() {
		assert_eq!(
			redact_secret_eth_node_endpoint(
				"wss://mainnet.infura.io/ws/v3/d52c362116b640b98a166d08d3170a42"
			)
			.unwrap(),
			"wss://mainnet.infura.io/ws/v3/d52****"
		);
		assert_eq!(
			redact_secret_eth_node_endpoint(
				"wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/"
			)
			.unwrap(),
			"wss://cdc****.rinkeby.ws.rivet.cloud/"
		);
		// same, but HTTP
		assert_eq!(
			redact_secret_eth_node_endpoint(
				"https://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.rpc.rivet.cloud/"
			)
			.unwrap(),
			"https://cdc****.rinkeby.rpc.rivet.cloud/"
		);
		assert_eq!(
			redact_secret_eth_node_endpoint("wss://non_32hex_secret.rinkeby.ws.rivet.cloud/")
				.unwrap(),
			"wss://non****"
		);
		assert_eq!(redact_secret_eth_node_endpoint("wss://a").unwrap(), "wss://a****");
		// same, but HTTP
		assert_eq!(redact_secret_eth_node_endpoint("http://a").unwrap(), "http://a****");
		assert!(redact_secret_eth_node_endpoint("no.schema.com").is_err());
	}
}
