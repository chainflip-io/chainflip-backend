//! Contains the information required to use the KeyManager contract as a source for
//! the EthEventStreamer

use crate::eth::EventParseError;
use crate::state_chain::client::StateChainClient;
use crate::{
    eth::{utils, SignatureAndEvent},
    state_chain::client::StateChainRpcApi,
};
use cf_chains::eth::SchnorrVerificationComponents;
use cf_traits::EpochIndex;
use std::sync::Arc;
use web3::{
    contract::tokens::Tokenizable,
    ethabi::{self, RawLog, Token},
    types::{H160, H256},
};

use anyhow::Result;

use std::fmt::Debug;

use async_trait::async_trait;

use super::event_common::EventWithCommon;
use super::DecodeLogClosure;
use super::EthObserver;

/// A wrapper for the KeyManager Ethereum contract.
pub struct KeyManager {
    pub deployed_address: H160,
    pub contract: ethabi::Contract,
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct ChainflipKey {
    pub_key_x: ethabi::Uint,
    pub_key_y_parity: ethabi::Uint,
}

impl ChainflipKey {
    /// Create a ChainflipKey from a decimal string
    pub fn from_dec_str(dec_str: &str, parity: bool) -> Result<Self> {
        let pub_key_x = web3::types::U256::from_dec_str(dec_str)?;
        Ok(ChainflipKey {
            pub_key_x,
            pub_key_y_parity: match parity {
                true => web3::types::U256::from(1),
                false => web3::types::U256::from(0),
            },
        })
    }

    /// 1 byte of pub_key_y_parity followed by 32 bytes of pub_key_x
    /// Equivalent to secp256k1::PublicKey.serialize()
    pub fn serialize(&self) -> [u8; 33] {
        let mut bytes: [u8; 33] = [0; 33];
        self.pub_key_x.to_big_endian(&mut bytes[1..]);
        bytes[0] = match self.pub_key_y_parity.is_zero() {
            true => 2,
            false => 3,
        };
        bytes
    }
}

impl Tokenizable for ChainflipKey {
    fn from_token(token: ethabi::Token) -> Result<Self, web3::contract::Error>
    where
        Self: Sized,
    {
        if let Token::Tuple(members) = token {
            if members.len() != 2 {
                Err(web3::contract::Error::InvalidOutputType(
                    stringify!(ChainflipKey).to_owned(),
                ))
            } else {
                Ok(ChainflipKey {
                    pub_key_x: ethabi::Uint::from_token(members[0].clone())?,
                    pub_key_y_parity: ethabi::Uint::from_token(members[1].clone())?,
                })
            }
        } else {
            Err(web3::contract::Error::InvalidOutputType(
                stringify!(ChainflipKey).to_owned(),
            ))
        }
    }

    fn into_token(self) -> ethabi::Token {
        Token::Tuple(vec![
            // Key
            Token::Uint(self.pub_key_x),
            Token::Uint(self.pub_key_y_parity),
        ])
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SigData {
    pub key_man_addr: ethabi::Address,
    pub chain_id: ethabi::Uint,
    pub msg_hash: ethabi::Uint,
    pub sig: ethabi::Uint,
    pub nonce: ethabi::Uint,
    pub k_times_g_address: ethabi::Address,
}

impl Tokenizable for SigData {
    fn from_token(token: ethabi::Token) -> Result<Self, web3::contract::Error>
    where
        Self: Sized,
    {
        if let Token::Tuple(members) = token {
            if members.len() != 6 {
                Err(web3::contract::Error::InvalidOutputType(
                    stringify!(SigData).to_owned(),
                ))
            } else {
                Ok(SigData {
                    key_man_addr: ethabi::Address::from_token(members[0].clone())?,
                    chain_id: ethabi::Uint::from_token(members[1].clone())?,
                    msg_hash: ethabi::Uint::from_token(members[2].clone())?,
                    sig: ethabi::Uint::from_token(members[3].clone())?,
                    nonce: ethabi::Uint::from_token(members[4].clone())?,
                    k_times_g_address: ethabi::Address::from_token(members[5].clone())?,
                })
            }
        } else {
            Err(web3::contract::Error::InvalidOutputType(
                stringify!(SigData).to_owned(),
            ))
        }
    }

    fn into_token(self) -> ethabi::Token {
        Token::Tuple(vec![
            // Key
            Token::Address(self.key_man_addr),
            Token::Uint(self.chain_id),
            Token::Uint(self.msg_hash),
            Token::Uint(self.sig),
            Token::Uint(self.nonce),
            Token::Address(self.k_times_g_address),
        ])
    }
}

/// Represents the events that are expected from the KeyManager contract.
#[derive(Debug, PartialEq)]
pub enum KeyManagerEvent {
    /// `AggKeySetByAggKey(Key oldKey, Key newKey)`
    AggKeySetByAggKey {
        /// The old key.
        old_key: ChainflipKey,
        /// The new key.
        new_key: ChainflipKey,
    },

    /// `AggKeySetByGovKey(Key oldKey, Key newKey)`
    AggKeySetByGovKey {
        /// The old key.
        old_key: ChainflipKey,
        /// The new key.
        new_key: ChainflipKey,
    },

    /// `GovKeySetByGovKey(Key oldKey, Key newKey)`
    GovKeySetByGovKey {
        /// The old key.
        old_key: ethabi::Address,
        /// The new key.
        new_key: ethabi::Address,
    },

    /// `SignatureAccepted(sigData, signer);`
    SignatureAccepted {
        /// Contains a signature and the msgHash that the signature is over. Kept as a single struct.
        sig_data: SigData,
        /// Address of the signer of the broadcast.
        signer: ethabi::Address,
    },
}

#[async_trait]
impl EthObserver for KeyManager {
    type EventParameters = KeyManagerEvent;

    fn contract_name(&self) -> &'static str {
        "KeyManager"
    }

    async fn handle_event<RpcClient>(
        &self,
        _epoch: EpochIndex,
        event: EventWithCommon<Self::EventParameters>,
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        logger: &slog::Logger,
    ) where
        RpcClient: 'static + StateChainRpcApi + Sync + Send,
    {
        slog::info!(logger, "Handling event: {}", event);
        match event.event_parameters {
            KeyManagerEvent::AggKeySetByAggKey { new_key, .. }
            | KeyManagerEvent::AggKeySetByGovKey { new_key, .. } => {
                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_witnesser::Call::witness {
                            call: Box::new(
                                pallet_cf_vaults::Call::vault_key_rotated {
                                    new_public_key: cf_chains::eth::AggKey::from_pubkey_compressed(
                                        new_key.serialize(),
                                    ),
                                    block_number: event.block_number,
                                    tx_hash: event.tx_hash,
                                }
                                .into(),
                            ),
                        },
                        logger,
                    )
                    .await;
            }
            KeyManagerEvent::SignatureAccepted { sig_data, signer } => {
                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_witnesser::Call::witness {
                            call: Box::new(
                                pallet_cf_broadcast::Call::signature_accepted {
                                    payload: SchnorrVerificationComponents {
                                        s: sig_data.sig.into(),
                                        k_times_g_address: sig_data.k_times_g_address.into(),
                                    },
                                    tx_signer: signer,
                                    block_number: event.block_number,
                                    tx_hash: event.tx_hash,
                                }
                                .into(),
                            ),
                        },
                        logger,
                    )
                    .await;
            }
            _ => {
                slog::trace!(logger, "Ignoring unused event: {}", event);
            }
        }
    }

    fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>> {
        let ak_set_by_ak = SignatureAndEvent::new(&self.contract, "AggKeySetByAggKey")?;
        let ak_set_by_gk = SignatureAndEvent::new(&self.contract, "AggKeySetByGovKey")?;
        let gk_set_by_gk = SignatureAndEvent::new(&self.contract, "GovKeySetByGovKey")?;
        let sig_accepted = SignatureAndEvent::new(&self.contract, "SignatureAccepted")?;

        Ok(Box::new(
            move |signature: H256, raw_log: RawLog| -> Result<KeyManagerEvent> {
                Ok(if signature == ak_set_by_ak.signature {
                    let log = ak_set_by_ak.event.parse_log(raw_log)?;
                    KeyManagerEvent::AggKeySetByAggKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                    }
                } else if signature == ak_set_by_gk.signature {
                    let log = ak_set_by_gk.event.parse_log(raw_log)?;
                    KeyManagerEvent::AggKeySetByGovKey {
                        old_key: utils::decode_log_param::<ChainflipKey>(&log, "oldKey")?,
                        new_key: utils::decode_log_param::<ChainflipKey>(&log, "newKey")?,
                    }
                } else if signature == gk_set_by_gk.signature {
                    let log = gk_set_by_gk.event.parse_log(raw_log)?;
                    KeyManagerEvent::GovKeySetByGovKey {
                        old_key: utils::decode_log_param(&log, "oldKey")?,
                        new_key: utils::decode_log_param(&log, "newKey")?,
                    }
                } else if signature == sig_accepted.signature {
                    let log = sig_accepted.event.parse_log(raw_log)?;
                    KeyManagerEvent::SignatureAccepted {
                        sig_data: utils::decode_log_param::<SigData>(&log, "sigData")?,
                        signer: utils::decode_log_param(&log, "signer")?,
                    }
                } else {
                    return Err(anyhow::anyhow!(EventParseError::UnexpectedEvent(signature)));
                })
            },
        ))
    }

    fn get_contract_address(&self) -> H160 {
        self.deployed_address
    }
}

impl KeyManager {
    /// Loads the contract abi to get the event definitions
    pub fn new(deployed_address: H160) -> Result<Self> {
        Ok(Self {
            deployed_address,
            contract: ethabi::Contract::load(std::include_bytes!("abis/KeyManager.json").as_ref())?,
        })
    }
}

#[cfg(test)]
mod tests {

    use crate::eth::EventParseError;

    use super::*;
    use hex;
    use std::str::FromStr;
    use web3::types::{H256, U256};

    // All log data for these tests was obtained from the events in the `deploy_and` script:
    // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/scripts/deploy_and.py

    // All the key strings in this test are decimal pub keys derived from the priv keys in the consts.py script
    // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py

    // 🔑 Aggregate Key sets the new Aggregate Key 🔑
    #[test]
    fn test_ak_set_by_ak_parsing() {
        let key_manager = KeyManager::new(H160::default()).unwrap();
        let decode_log = key_manager.decode_log_closure().unwrap();
        let event_signature =
            H256::from_str("0x5cba64f32f2576e404f74394dc04611cce7416e299c94db0667d4e315e852521")
                .unwrap();

        match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("31b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing KeyManagerEvent::AggKeySetByAggKey event") {
                KeyManagerEvent::AggKeySetByAggKey {
                    old_key,
                    new_key,
                } => {
                    assert_eq!(old_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::AggKeySetByAggKey, got different variant"),
            }
    }

    // 🔑 Governance Key sets the new Aggregate Key 🔑
    #[test]
    fn test_ak_set_gk_parsing() {
        let key_manager = KeyManager::new(H160::default()).unwrap();
        let decode_log = key_manager.decode_log_closure().unwrap();
        let event_signature =
            H256::from_str("0xe441a6cf7a12870075eb2f6399c0de122bfe6cd8a75bfa83b05d5b611552532e")
                .unwrap();

        match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            ).expect("Failed parsing KeyManagerEvent::AggKeySetByGovKey event")
            {
                KeyManagerEvent::AggKeySetByGovKey {
                    old_key,
                    new_key,
                } => {
                    assert_eq!(old_key, ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                    assert_eq!(new_key, ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::AggKeySetByGovKey, got different variant"),
            }
    }

    // 🔑 Governance Key sets the new Governance Key 🔑
    #[test]
    fn test_gk_set_by_gk_parsing() {
        let key_manager = KeyManager::new(H160::default()).unwrap();
        let decode_log = key_manager.decode_log_closure().unwrap();
        let event_signature =
            H256::from_str("0xb79780665df55038fba66988b1b3f2eda919a59b75cd2581f31f8f04f58bec7c")
                .unwrap();

        match decode_log(
                event_signature,
                RawLog {
                    topics : vec![event_signature],
                    data : hex::decode("000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb922660000000000000000000000009965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap()
                }
            ).expect("Failed parsing KeyManagerEvent::GovKeySetByGovKey event")
            {
                KeyManagerEvent::GovKeySetByGovKey {
                    old_key,
                    new_key,
                } => {
                    assert_eq!(old_key, H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap());
                    assert_eq!(new_key, H160::from_str("0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc").unwrap());
                }
                _ => panic!("Expected KeyManagerEvent::GovKeySetByGovKey, got different variant"),
            }
    }

    #[test]
    fn test_sig_accepted_parsing() {
        let key_manager = KeyManager::new(H160::default()).unwrap();
        let decode_log = key_manager.decode_log_closure().unwrap();
        let event_signature =
            H256::from_str("0x38045dba3d9ee1fee641ad521bd1cf34c28562f6658772ee04678edf17b9a3bc")
                .unwrap();

        match decode_log(
            event_signature,
            RawLog {
                topics: vec![event_signature],
                data: hex::decode(
                    "000000000000000000000000e7f1725e7734ce288f8367e1bb143e90bb3f05120000000000000000000000000000000000000000000000000000000000007a69b918a2687d109fa0308fedb39f0dd091accd9edb80a9ddb2ccb1f0abaa6cfb64ed5ecfedaacc9bd0bcc5512e7fcf9650de5619acc0a747681f58d26f66468e7000000000000000000000000000000000000000000000000000000000000000030000000000000000000000007ceb2425ec324348ba69bd50205b11e29770fd96000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb92266",
                )
                .unwrap(),
            },
        )
        .expect("Failed parsing KeyManagerEvent::SignatureAccepted event")
        {
            KeyManagerEvent::SignatureAccepted {
                sig_data,
                signer,
            } => {
                assert_eq!(sig_data, SigData{
                    key_man_addr: H160::from_str("0xe7f1725e7734ce288f8367e1bb143e90bb3f0512").unwrap(),
                    chain_id: U256::from_dec_str("31337").unwrap(),
                    msg_hash: U256::from_dec_str("83721402217372471513450062042778477963861354613529233808466400078111064259428").unwrap(),
                    sig: U256::from_dec_str("107365663807311708634605056423336732647043554150507905924516852373709157469808").unwrap(),
                    nonce: U256::from_dec_str("3").unwrap(),
                    k_times_g_address: H160::from_str("0x7ceb2425ec324348ba69bd50205b11e29770fd96").unwrap(),
                });
                assert_eq!(signer, H160::from_str("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266").unwrap());
            }
            _ => panic!("Expected KeyManagerEvent::SignatureAccepted, got different variant"),
        }
    }

    #[test]
    fn test_invalid_sig() {
        let key_manager = KeyManager::new(H160::default()).unwrap();
        let decode_log = key_manager.decode_log_closure().unwrap();
        let invalid_signature =
            H256::from_str("0x0b0b5ed18390ab49777844d5fcafb9865c74095ceb3e73cc57d1fbcc926103b5")
                .unwrap();

        let res = decode_log(
                invalid_signature,
                RawLog {
                    topics : vec![invalid_signature],
                    data : hex::decode("000000000000000000000000000000000000000000000000000000000000000131b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()
                }
            )
            .map_err(|e| match e.downcast_ref::<EventParseError>() {
                Some(EventParseError::UnexpectedEvent(_)) => {}
                _ => {
                    panic!("Incorrect error parsing INVALID_SIG_LOG");
                }
            });
        assert!(res.is_err());
    }

    #[test]
    fn test_chainflip_key_serialize() {
        use secp256k1::PublicKey;

        // Create a `ChainflipKey` and a `PublicKey` that are the same
        let cf_key = ChainflipKey::from_dec_str(
            "22479114112312168431982914496826057754130808976066989807481484372215659188398",
            true,
        )
        .unwrap();

        let sk = secp256k1::SecretKey::from_str(
            "fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba",
        )
        .unwrap();

        let secp_key = PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk);

        // Compare the serialize() values to make sure we serialize the same as secp256k1
        assert_eq!(cf_key.serialize(), secp_key.serialize());
    }
}
