use regex::Regex;
use serde::Deserialize;
use std::fmt::{Debug, Display};
use url::Url;

const MAX_SECRET_CHARACTERS_REVEALED: usize = 3;
const SCHEMA_PADDING_LEN: usize = 3;

/// A wrapper around a `String` that redacts a secret in the url when displayed. Used for node
/// endpoints.
#[derive(Clone, PartialEq, Eq, Deserialize, Default)]
pub struct SecretUrl(String);

impl Display for SecretUrl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", redact_secret_node_endpoint(&self.0))
	}
}

// Only debug print the secret in debug mode
#[cfg(debug_assertions)]
impl Debug for SecretUrl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self.0)
	}
}
#[cfg(not(debug_assertions))]
impl Debug for SecretUrl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", redact_secret_node_endpoint(&self.0))
	}
}

impl From<String> for SecretUrl {
	fn from(s: String) -> Self {
		SecretUrl(s)
	}
}

impl<'a> From<&'a str> for SecretUrl {
	fn from(s: &'a str) -> Self {
		SecretUrl(s.to_string())
	}
}

impl From<SecretUrl> for String {
	fn from(s: SecretUrl) -> Self {
		s.0
	}
}

impl<'a> From<&'a SecretUrl> for &'a str {
	fn from(s: &'a SecretUrl) -> Self {
		&s.0
	}
}

impl AsRef<str> for SecretUrl {
	fn as_ref(&self) -> &str {
		&self.0
	}
}

/// Partially redacts the secret in the url of the node endpoint.
///  eg: `wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/` ->
/// `wss://cdc****.rinkeby.ws.rivet.cloud/`
#[allow(unused)]
pub fn redact_secret_node_endpoint(endpoint: &str) -> String {
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
		endpoint_redacted
	} else {
		// No secret was found, so just redact almost all of the url
		if let Ok(url) = Url::parse(endpoint) {
			format!(
				"{}****",
				endpoint
					.split_at(usize::min(
						url.scheme().len() + SCHEMA_PADDING_LEN + MAX_SECRET_CHARACTERS_REVEALED,
						endpoint.len()
					))
					.0
			)
		} else {
			// Not a valid url, so just redact most of the string
			format!(
				"{}****",
				endpoint.split_at(usize::min(MAX_SECRET_CHARACTERS_REVEALED, endpoint.len())).0
			)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_secret_web_addresses() {
		assert_eq!(
			format!(
				"{}",
				SecretUrl(
					"wss://mainnet.infura.io/ws/v3/d52c362116b640b98a166d08d3170a42".to_string()
				)
			),
			"wss://mainnet.infura.io/ws/v3/d52****"
		);
		assert_eq!(
			format!(
				"{}",
				SecretUrl(
					"wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/".to_string()
				)
			),
			"wss://cdc****.rinkeby.ws.rivet.cloud/"
		);
		assert_eq!(
			format!("{}", SecretUrl("wss://non_32hex_secret.rinkeby.ws.rivet.cloud/".to_string())),
			"wss://non****"
		);
		assert_eq!(format!("{}", SecretUrl("wss://a".to_string())), "wss://a****");

		// Same, but HTTPS
		assert_eq!(
			format!(
				"{}",
				SecretUrl(
					"https://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.rpc.rivet.cloud/".to_string()
				)
			),
			"https://cdc****.rinkeby.rpc.rivet.cloud/"
		);
		assert_eq!(
			format!(
				"{}",
				SecretUrl("https://non_32hex_secret.rinkeby.ws.rivet.cloud/".to_string())
			),
			"https://non****"
		);
		assert_eq!(format!("{}", SecretUrl("https://a".to_string())), "https://a****");

		assert_eq!(format!("{}", SecretUrl("no.schema.com".to_string())), "no.****");
		assert_eq!(format!("{:?}", SecretUrl("debug_print".to_string())), "\"debug_print\"");
	}
}
