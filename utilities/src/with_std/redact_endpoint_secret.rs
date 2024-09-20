use regex::Regex;
use serde::Deserialize;
use core::str::FromStr;
use std::fmt::{Debug, Display};
use url::Url;

const MAX_SECRET_CHARACTERS_REVEALED: usize = 3;
const SCHEMA_PADDING_LEN: usize = 3;

/// A wrapper around a `Url` that redacts a secret in the url when displayed. Used for node
/// endpoints.
#[derive(Clone, PartialEq, Deserialize, Eq)]
pub struct SecretUrl(Url);

impl Display for SecretUrl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", redact_secret_endpoint(self.0.as_ref()))
	}
}

impl Debug for SecretUrl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// Only debug print the secret without redaction in debug mode
		if cfg!(debug_assertions) {
			write!(f, "{:?}", self.0)
		} else {
			write!(f, "{:?}", redact_secret_endpoint(self.0.as_ref()))
		}
	}
}

impl From<Url> for SecretUrl {
	fn from(value: Url) -> Self {
		SecretUrl(value)
	}
}

impl From<SecretUrl> for Url {
	fn from(value: SecretUrl) -> Self {
		value.0
	}
}

impl FromStr for SecretUrl {
	type Err = url::ParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		s.parse().map(SecretUrl)
	}
}

impl From<SecretUrl> for String {
	fn from(s: SecretUrl) -> Self {
		s.0.into()
	}
}

impl AsRef<str> for SecretUrl {
	fn as_ref(&self) -> &str {
		self.0.as_ref()
	}
}

/// Partially redacts the secret in the url of the node endpoint.
///  eg: `wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/` ->
/// `wss://cdc****.rinkeby.ws.rivet.cloud/`
pub fn redact_secret_endpoint(endpoint: &str) -> String {
	const REGEX_ETH_SECRET: &str = "[0-9a-fA-F]{32}";
	const REGEX_BTC_SECRET: &str =
		r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}";
	let re = Regex::new(&format!("({})|({})", REGEX_ETH_SECRET, REGEX_BTC_SECRET)).unwrap();
	if re.is_match(endpoint) {
		// A 32 character hex string was found, redact it
		let mut endpoint_redacted = endpoint.to_string();
		// Just redact the first match so we do not get stuck in a loop if there is a mistake in the
		// regex
		if let Some(capture) = re.captures_iter(endpoint).next() {
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
					"wss://mainnet.infura.io/ws/v3/d52c362116b640b98a166d08d3170a42".parse().unwrap()
				)
			),
			"wss://mainnet.infura.io/ws/v3/d52****"
		);
		assert_eq!(
			format!(
				"{}",
				SecretUrl(
					"wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/".parse().unwrap()
				)
			),
			"wss://cdc****.rinkeby.ws.rivet.cloud/"
		);
		assert_eq!(
			format!("{}", SecretUrl("wss://non_32hex_secret.rinkeby.ws.rivet.cloud/".parse().unwrap())),
			"wss://non****"
		);
		assert_eq!(format!("{}", SecretUrl("wss://a".parse().unwrap())), "wss://a****");

		// Same, but HTTPS
		assert_eq!(
			format!(
				"{}",
				SecretUrl(
					"https://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.rpc.rivet.cloud/".parse().unwrap()
				)
			),
			"https://cdc****.rinkeby.rpc.rivet.cloud/"
		);
		assert_eq!(
			format!(
				"{}",
				SecretUrl("https://non_32hex_secret.rinkeby.ws.rivet.cloud/".parse().unwrap())
			),
			"https://non****"
		);
		assert_eq!(format!("{}", SecretUrl("https://a".parse().unwrap())), "https://a****");

		assert_eq!(format!("{}", SecretUrl("no.schema.com".parse().unwrap())), "no.****");
		if cfg!(debug_assertions) {
			assert_eq!(format!("{:?}", SecretUrl("debug_print".parse().unwrap())), "\"debug_print\"");
		} else {
			assert_eq!(format!("{:?}", SecretUrl("debug_print".parse().unwrap())), "\"deb****\"");
		}

		assert_eq!(
			format!(
				"{}",
				SecretUrl(
					"btc.getblock.io/de76678e-a489-4503-2ba2-81156c471220/mainnet".parse().unwrap()
				)
			),
			"btc.getblock.io/de7****/mainnet"
		);
	}
}
