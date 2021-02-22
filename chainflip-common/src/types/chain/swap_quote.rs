use super::{UniqueId, Validate};
use crate::{
    string::*,
    types::{
        coin::Coin, fraction::PercentageFraction, unique_id::GetUniqueId, utf8::ByteString, Bytes,
        Network, Timestamp,
    },
    validation::{validate_address, validate_address_id},
};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapQuote {
    /// Creation timestamp
    pub timestamp: Timestamp,
    /// The input coin
    pub input: Coin,
    /// The address in which the user will deposit coins
    pub input_address: ByteString,
    /// The information used to derive `input_address`
    pub input_address_id: Bytes,
    /// The address to refund coins in the case of a failed swap
    ///
    /// Invariant: Must be set if `slippage_limit` is set.
    pub return_address: Option<ByteString>,
    /// The output coin
    pub output: Coin,
    /// The address in which the user will receive coins
    pub output_address: ByteString,
    /// The ratio between the input amount and output amounts at the time of quote creation.
    /// Stored as: `(input_amount << 64) / estimated_output_amount`
    pub effective_price: u128,
    /// The maximum price slippage limit
    ///
    /// Invariant: `return_address` must be set if `slippage_limit` is set.
    pub slippage_limit: Option<PercentageFraction>,
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl Validate for SwapQuote {
    type Error = &'static str;

    fn validate(&self, network: Network) -> Result<(), Self::Error> {
        if self.input == self.output {
            return Err("Input and Output cannot be the same coin");
        }

        validate_address(self.input, network, &self.input_address.to_string())
            .map_err(|_| "Invalid input address")?;
        validate_address_id(self.input, &self.input_address_id)
            .map_err(|_| "Invalid input address id")?;

        if let Some(address) = &self.return_address {
            validate_address(self.input, network, &address.to_string())
                .map_err(|_| "Invalid return address")?;
        }

        validate_address(self.output, network, &self.output_address.to_string())
            .map_err(|_| "Invalid output address")?;

        if (self.slippage_limit.is_some() || self.input.get_info().requires_return_address)
            && self.return_address.is_none()
        {
            return Err("Return address required");
        }

        Ok(())
    }
}

// Used as a key in the KV store of CFE
impl Hash for SwapQuote {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.input.hash(state);
        self.output.hash(state);
        self.input_address.hash(state);
    }
}

impl GetUniqueId for SwapQuote {
    type UniqueId = UniqueId;

    fn unique_id(&self) -> Self::UniqueId {
        let mut s = SipHasher::new();
        self.hash(&mut s);
        s.finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::constants::{ETH_ADDRESS, OXEN_ADDRESS, OXEN_PAYMENT_ID};
    use std::str::FromStr;

    fn quote() -> SwapQuote {
        SwapQuote {
            timestamp: Timestamp(0),
            input: Coin::ETH,
            input_address: ETH_ADDRESS.into(),
            input_address_id: vec![0; 32],
            return_address: None,
            output: Coin::OXEN,
            output_address: OXEN_ADDRESS.into(),
            effective_price: 0,
            slippage_limit: None,
            event_number: None,
        }
    }

    #[test]
    fn validates() {
        let swap = quote();
        assert!(swap.validate(Network::Mainnet).is_ok());

        let mut swap = quote();
        swap.output = swap.input;
        assert_eq!(
            swap.validate(Network::Mainnet).unwrap_err(),
            "Input and Output cannot be the same coin"
        );

        let mut swap = quote();
        swap.input_address = "invalid".into();
        assert_eq!(
            swap.validate(Network::Mainnet).unwrap_err(),
            "Invalid input address"
        );

        let mut swap = quote();
        swap.input_address_id = vec![];
        assert_eq!(
            swap.validate(Network::Mainnet).unwrap_err(),
            "Invalid input address id"
        );

        let mut swap = quote();
        swap.return_address = Some("invalid".into());
        assert_eq!(
            swap.validate(Network::Mainnet).unwrap_err(),
            "Invalid return address"
        );

        let mut swap = quote();
        swap.output_address = "invalid".into();
        assert_eq!(
            swap.validate(Network::Mainnet).unwrap_err(),
            "Invalid output address"
        );

        let mut swap = quote();
        swap.return_address = None;
        swap.slippage_limit = Some(PercentageFraction::from_str("0.1").unwrap());
        assert_eq!(
            swap.validate(Network::Mainnet).unwrap_err(),
            "Return address required"
        );

        let mut swap = quote();
        swap.input = Coin::OXEN;
        swap.input_address = OXEN_ADDRESS.into();
        swap.input_address_id = OXEN_PAYMENT_ID.bytes();
        swap.return_address = None;
        swap.slippage_limit = None;
        swap.output = Coin::ETH;
        swap.output_address = ETH_ADDRESS.into();
        assert_eq!(
            swap.validate(Network::Mainnet).unwrap_err(),
            "Return address required"
        );
    }

    #[test]
    fn hash_swap_quote() {
        let output_sent = quote();
        let mut s = SipHasher::new();
        output_sent.hash(&mut s);
        let hash = s.finish();

        assert_eq!(3898344093373112451, hash);
    }
}
