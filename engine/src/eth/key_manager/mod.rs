//! KeyManager eth contract wrapper and utilities.

use core::str::FromStr;

use serde::{Deserialize, Serialize};
use web3::{contract::tokens::Tokenizable, ethabi::{self, Function, Token}, types::{H160}};

use anyhow::Result;

#[derive(Clone)]
/// A wrapper for the KeyManager Ethereum contract.
pub struct KeyManager {
    pub deployed_address: H160,
    contract: ethabi::Contract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainflipKey {
    pub_key_x: ethabi::Uint,
    pub_key_y_parity: ethabi::Uint,
    nonce_times_g_addr: ethabi::Address,
}

impl Tokenizable for ChainflipKey {
    fn from_token(token: ethabi::Token) -> Result<Self, web3::contract::Error>
    where
        Self: Sized,
    {
        if let Token::Tuple(members) = token {
            if members.len() != 3 {
                Err(web3::contract::Error::InvalidOutputType(stringify!(ChainflipKey).to_owned()))
            } else {
                Ok(ChainflipKey {
                    pub_key_x: ethabi::Uint::from_token(members[0].clone())?,
                    pub_key_y_parity: ethabi::Uint::from_token(members[1].clone())?,
                    nonce_times_g_addr: ethabi::Address::from_token(members[2].clone())?,
                })
            }
        } else {
            Err(web3::contract::Error::InvalidOutputType(stringify!(ChainflipKey).to_owned()))
        }
    }

    fn into_token(self) -> ethabi::Token {
        Token::Tuple(vec![ // Key
            Token::Uint(self.pub_key_x), // msgHash
            Token::Uint(self.pub_key_y_parity),// nonce
            Token::Address(self.nonce_times_g_addr) // sig
        ])
    }
}

/// Represents the events that are expected from the KeyManager contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyManagerEvent {
    /// The `Staked(nodeId, amount)` event.
    KeyChange(
        /// Whether the change was signed by the AggKey.
        bool,
        /// The old key.
        ChainflipKey,
        /// The new key.
        ChainflipKey,
    ),
}

impl KeyManager {
    /// Loads the contract abi to get event definitions
    pub fn load(deployed_address: &str) -> Result<Self> {
        let abi_bytes = std::include_bytes!("../abis/KeyManager.json");
        let contract = ethabi::Contract::load(abi_bytes.as_ref())?;

        Ok(Self {
            deployed_address: H160::from_str(deployed_address)?,
            contract,
        })
    }

    /// Extracts a reference to the "setAggKeyWithAggKey" function definition. Panics if it can't be found.
    pub fn set_agg_key_with_agg_key(&self) -> &Function {
        self.contract
            .function("setAggKeyWithAggKey")
            .expect("Function 'setAggKeyWithAggKey' should be defined in the KeyManager abi.")
    }
}
