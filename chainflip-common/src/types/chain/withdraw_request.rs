use super::{UniqueId, Validate};
use crate::{
    string::*,
    types::{
        coin::Coin, fraction::WithdrawFraction, unique_id::GetUniqueId, utf8::ByteString, Bytes,
        Network, Timestamp,
    },
    validation::{validate_address, validate_staker_id},
};
use codec::{Decode, Encode};
use ring::{
    rand,
    signature::{self, EcdsaKeyPair, VerificationAlgorithm},
};
use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawRequest {
    /// Creation timestamp
    pub timestamp: Timestamp,
    /// The staker public key.
    pub staker_id: Bytes,
    /// The pool in which to withdraw.
    pub pool: Coin,
    /// The address to withdraw the base coins to.
    pub base_address: ByteString,
    /// The address to withdraw the other coins to.
    pub other_address: ByteString,
    /// Fraction of the total portions to withdraw
    pub fraction: WithdrawFraction,
    /// ECDSA-P256-SHA256 Signature
    pub signature: Bytes,
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl WithdrawRequest {
    fn serialize(&self) -> Bytes {
        format!(
            "{}|{}|{}|{}|{}|{}",
            hex::encode(self.staker_id.clone()),
            self.pool,
            self.base_address,
            self.other_address,
            self.fraction,
            self.timestamp.0
        )
        .as_bytes()
        .into()
    }

    /// Get the signature of this request
    pub fn signature(&self, keys: &EcdsaKeyPair) -> Result<Bytes, ()> {
        let rng = rand::SystemRandom::new();
        let message = self.serialize();
        let sig = keys.sign(&rng, &message).map_err(|_| ())?;
        Ok(sig.as_ref().into())
    }

    /// Sign this withdraw request
    pub fn sign(&mut self, keys: &EcdsaKeyPair) -> Result<(), ()> {
        let signature = self.signature(keys)?;
        self.signature = signature;
        Ok(())
    }

    /// Verify this transaction
    pub fn verify_signature(&self) -> bool {
        let pubkey: &[u8] = &self.staker_id;
        let signed_data: &[u8] = &self.serialize();
        let signature: &[u8] = &self.signature;

        match signature::ECDSA_P256_SHA256_FIXED.verify(
            pubkey.into(),
            signed_data.into(),
            signature.into(),
        ) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

impl Validate for WithdrawRequest {
    type Error = &'static str;

    fn validate(&self, network: Network) -> Result<(), Self::Error> {
        if validate_staker_id(&self.staker_id).is_err() {
            return Err("Invalid staker id");
        }

        if self.pool == Coin::BASE_COIN {
            return Err("Invalid pool coin");
        }

        validate_address(Coin::BASE_COIN, network, &self.base_address.to_string())
            .map_err(|_| "Invalid base address")?;
        validate_address(self.pool, network, &self.other_address.to_string())
            .map_err(|_| "Invalid other address")?;

        if !self.verify_signature() {
            return Err("Invalid signature");
        }

        Ok(())
    }
}

// Used as a key in the KV store of CFE
// Eventually we'll have to use block# and block_index to ensure this is unqiue and
// nodes are able to come to consensus on the order of quotes
// Currently there's a pretty high chance this won't be unique
impl Hash for WithdrawRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pool.hash(state);
        self.base_address.hash(state);
        self.other_address.hash(state);
    }
}

impl GetUniqueId for WithdrawRequest {
    type UniqueId = UniqueId;

    fn unique_id(&self) -> Self::UniqueId {
        let mut s = SipHasher::new();
        self.hash(&mut s);
        s.finish()
    }
}

#[cfg(test)]
mod test {
    use ring::signature::KeyPair;

    use crate::test::constants::{ETH_ADDRESS, OXEN_ADDRESS};

    use super::*;

    fn request() -> WithdrawRequest {
        WithdrawRequest {
            timestamp: Timestamp(1605584285796),
            staker_id: hex::decode("0433829aa2cccda485ee215421bd6c2af3e6e1702e3202790af42a7332c3fc06ec08beafef0b504ed20d5176f6323da3a4d34c5761a82487087d93ebd673ca7293").unwrap(),
            pool: Coin::BTC,
            base_address: "T6SiGp1EuAB5qE8jqdMu7pHr8Tw5DcQtKT34MxjxY5tjUyzZ3QSHcsS78fVw4B2iKqgqnfB2H1Sac5BG1yWD7NLq2Q41A7EqV".into(),
            other_address: "tb1qdnqrpkua5tv9u7h50d3vaz64sd49fyxvf5tcxa".into(),
            fraction: WithdrawFraction::MAX,
            signature: base64::decode("nK54pz5uCox6udLt3JgCoB0PYp59DVpRH3/i6fxKPhYO+0OiQwOi0/xCv5261h4SIU/h2ODsNTs2XDDLJgK+MA==").unwrap(),
            event_number: None
        }
    }

    /// Get fake ecdsa keypiar used for signing unstake txs
    fn get_fake_ecdsa_key() -> EcdsaKeyPair {
        let algo = &signature::ECDSA_P256_SHA256_FIXED_SIGNING;

        let bytes: Vec<u8> = vec![
            48, 129, 135, 2, 1, 0, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72,
            206, 61, 3, 1, 7, 4, 109, 48, 107, 2, 1, 1, 4, 32, 161, 231, 12, 64, 10, 98, 188, 142,
            95, 151, 41, 75, 22, 45, 167, 228, 199, 84, 182, 50, 7, 167, 152, 143, 58, 184, 72, 26,
            229, 154, 192, 79, 161, 68, 3, 66, 0, 4, 51, 130, 154, 162, 204, 205, 164, 133, 238,
            33, 84, 33, 189, 108, 42, 243, 230, 225, 112, 46, 50, 2, 121, 10, 244, 42, 115, 50,
            195, 252, 6, 236, 8, 190, 175, 239, 11, 80, 78, 210, 13, 81, 118, 246, 50, 61, 163,
            164, 211, 76, 87, 97, 168, 36, 135, 8, 125, 147, 235, 214, 115, 202, 114, 147,
        ];

        EcdsaKeyPair::from_pkcs8(algo, &bytes).unwrap()
    }

    #[test]
    fn sign_and_verify() {
        let data = request();
        assert_eq!(data.verify_signature(), true);

        let keys = get_fake_ecdsa_key();

        let mut data = WithdrawRequest {
            timestamp: Timestamp(0),
            staker_id: keys.public_key().as_ref().to_vec(),
            pool: Coin::ETH,
            base_address: OXEN_ADDRESS.into(),
            other_address: ETH_ADDRESS.into(),
            fraction: WithdrawFraction::MAX,
            signature: vec![],
            event_number: None,
        };

        let signed = data.sign(&keys);
        assert!(signed.is_ok());
        assert_eq!(data.verify_signature(), true);

        // Changing any part of the data should invalidate it
        data.staker_id = hex::decode("0433829aa2cccda485ee215421326c2af3e6e1702e3202790af42a7332c3fc06ec08beafef0b504ed20d5176f6323da3a4d34c5761a82487087d93ebd673ca7293").unwrap();
        assert_eq!(data.verify_signature(), false);
    }

    #[test]
    fn serialisation() {
        let keys = get_fake_ecdsa_key();
        let data = WithdrawRequest {
            timestamp: Timestamp(1603777110013u128),
            staker_id: keys.public_key().as_ref().to_vec(),
            pool: Coin::ETH,
            base_address: OXEN_ADDRESS.into(),
            other_address: ETH_ADDRESS.into(),
            fraction: WithdrawFraction::MAX,
            signature: vec![],
            event_number: None,
        };

        let expected = "0433829aa2cccda485ee215421bd6c2af3e6e1702e3202790af42a7332c3fc06ec08beafef0b504ed20d5176f6323da3a4d34c5761a82487087d93ebd673ca7293|ETH|LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf|0x70E7Db0678460C5e53F1FFc9221d1C692111dCc5|10000|1603777110013".as_bytes();
        assert_eq!(data.serialize(), expected);
    }

    #[test]
    fn validates() {
        let data = request();
        data.validate(Network::Testnet).unwrap();

        let mut data = request();
        data.staker_id = vec![];
        assert_eq!(
            data.validate(Network::Testnet).unwrap_err(),
            "Invalid staker id"
        );

        let mut data = request();
        data.pool = Coin::BASE_COIN;
        assert_eq!(
            data.validate(Network::Testnet).unwrap_err(),
            "Invalid pool coin"
        );

        let mut data = request();
        data.base_address = "Invalid".into();
        assert_eq!(
            data.validate(Network::Testnet).unwrap_err(),
            "Invalid base address"
        );

        let mut data = request();
        data.other_address = "Invalid".into();
        assert_eq!(
            data.validate(Network::Testnet).unwrap_err(),
            "Invalid other address"
        );

        let mut data = request();
        data.signature = vec![];
        assert_eq!(
            data.validate(Network::Testnet).unwrap_err(),
            "Invalid signature"
        );
    }

    #[test]
    fn hash_withdraw_request() {
        let withdraw_request = request();
        let mut s = SipHasher::new();
        withdraw_request.hash(&mut s);
        let hash = s.finish();

        assert_eq!(17655162870837722522, hash);
    }
}
