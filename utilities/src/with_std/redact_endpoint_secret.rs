use core::net::IpAddr;
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
		write!(f, "{}", redact_secret_endpoint(&self.0))
	}
}

impl Debug for SecretUrl {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// Only debug print the secret without redaction in debug mode
		if cfg!(debug_assertions) {
			write!(f, "{:?}", self.0)
		} else {
			write!(f, "{:?}", redact_secret_endpoint(&self.0))
		}
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

impl AsRef<str> for SecretUrl {
	fn as_ref(&self) -> &str {
		&self.0
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
		// A secret was found, redact it
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
		// No secret was found
		if let Ok(url) = Url::parse(endpoint) {
			if is_local_url(&url) {
				// Don't redact anything if it is a local address
				endpoint.to_string()
			} else {
				// Redact almost all of the url
				format!(
					"{}****",
					endpoint
						.split_at(usize::min(
							url.scheme().len() +
								SCHEMA_PADDING_LEN + MAX_SECRET_CHARACTERS_REVEALED,
							endpoint.len()
						))
						.0
				)
			}
		} else {
			// Not a valid url, so just redact most of the string
			format!(
				"{}****",
				endpoint.split_at(usize::min(MAX_SECRET_CHARACTERS_REVEALED, endpoint.len())).0
			)
		}
	}
}

fn is_local_url(url: &Url) -> bool {
	match url.host_str() {
		Some("localhost") => true,
		Some(host) => match host.parse::<IpAddr>() {
			Ok(IpAddr::V4(ipv4)) => ipv4.is_loopback() || ipv4.is_private(),
			Ok(IpAddr::V6(ipv6)) => ipv6.is_loopback() || ipv6.is_unique_local(),
			_ => false,
		},
		None => false,
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
		if cfg!(debug_assertions) {
			assert_eq!(format!("{:?}", SecretUrl("debug_print".to_string())), "\"debug_print\"");
		} else {
			assert_eq!(format!("{:?}", SecretUrl("debug_print".to_string())), "\"deb****\"");
		}

		assert_eq!(
			format!(
				"{}",
				SecretUrl(
					"btc.getblock.io/de76678e-a489-4503-2ba2-81156c471220/mainnet".to_string()
				)
			),
			"btc.getblock.io/de7****/mainnet"
		);

		assert_eq!(
			format!(
				"{}",
				SecretUrl("wss://192.168.0.123/ws/v3/d52c362116b640b98a166d08d3170a42".to_string())
			),
			"wss://192.168.0.123/ws/v3/d52****"
		);

		// Local addresses without secrets should not be redacted
		assert_eq!(
			format!("{}", SecretUrl("ws://10.0.0.17".to_string())),
			"ws://10.0.0.17".to_string()
		);
		assert_eq!(
			format!("{}", SecretUrl("wss://127.0.0.1".to_string())),
			"wss://127.0.0.1".to_string()
		);
		assert_eq!(
			format!("{}", SecretUrl("https://192.168.0.123".to_string())),
			"https://192.168.0.123".to_string()
		);
		assert_eq!(
			format!("{}", SecretUrl("http://localhost".to_string())),
			"http://localhost".to_string()
		);
	}
}
