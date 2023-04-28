use crate::eth::EventParseError;
use anyhow::Result;
use web3::{contract::tokens::Tokenizable, ethabi::Log};

/// Helper method to decode the parameters from an ETH log
pub fn decode_log_param<T: Tokenizable>(log: &Log, param_name: &str) -> Result<T> {
	let token = &log
		.params
		.iter()
		.find(|&p| p.name == param_name)
		.ok_or_else(|| EventParseError::MissingParam(String::from(param_name)))?
		.value;

	Ok(Tokenizable::from_token(token.clone())?)
}
