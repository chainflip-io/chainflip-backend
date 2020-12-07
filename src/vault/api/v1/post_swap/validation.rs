use super::SwapQuoteParams;
use crate::{
    common::fractions::PercentageFraction,
    utils::validation::{validate_address, validate_address_id},
};

/// Validate quote params
pub fn validate_params(params: &SwapQuoteParams) -> Result<(), &'static str> {
    // Coins
    if !params.input_coin.is_supported() {
        return Err("Input coin is not supported");
    } else if !params.output_coin.is_supported() {
        return Err("Output coin is not supported");
    }

    if params.input_coin == params.output_coin {
        return Err("Cannot swap between the same coins");
    }

    // Amount

    let input_amount = params.input_amount.parse::<i128>().unwrap_or(0);
    if input_amount <= 0 {
        return Err("Invalid input amount provided");
    }

    // Addresses

    if params.input_coin.get_info().requires_return_address && params.input_return_address.is_none()
    {
        return Err("Input return address not provided");
    }

    if let Some(return_address) = &params.input_return_address {
        if validate_address(params.input_coin, &return_address).is_err() {
            return Err("Invalid return address");
        }
    }

    if validate_address(params.output_coin, &params.output_address).is_err() {
        return Err("Invalid output address");
    }

    if validate_address_id(params.input_coin, &params.input_address_id).is_err() {
        return Err("Invalid input id provided");
    }

    // Slippage

    if params.slippage_limit >= PercentageFraction::MAX.value() {
        return Err("Slippage limit must be between 0 and 10000");
    }

    if params.slippage_limit > 0 && params.input_return_address.is_none() {
        return Err("Input return address not provided");
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::common::Coin;

    use super::*;
    use std::collections::HashMap;

    const LOKI_ADDRESS: &str = "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY";
    const ETH_ADDRESS: &str = "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5";

    struct Values<T> {
        invalid: Vec<T>,
        valid: Vec<T>,
    }

    fn get_valid_params() -> SwapQuoteParams {
        SwapQuoteParams {
            input_coin: Coin::LOKI,
            input_return_address: Some(LOKI_ADDRESS.to_string()),
            input_address_id: "60900e5603bf96e3".to_owned(),
            input_amount: "1000000000".to_string(),
            output_coin: Coin::ETH,
            output_address: ETH_ADDRESS.to_string(),
            slippage_limit: 0,
        }
    }

    #[test]
    fn validates_correctly() {
        let valid = get_valid_params();
        assert_eq!(validate_params(&valid), Ok(()));
    }

    #[test]
    fn validates_coins() {
        let mut invalid = get_valid_params();
        invalid.input_coin = Coin::ETH;
        invalid.input_return_address = Some(invalid.output_address.clone());

        assert_eq!(
            validate_params(&invalid).unwrap_err(),
            "Cannot swap between the same coins"
        );
    }

    #[test]
    fn validates_input_amount() {
        let invalid_input_amounts = ["-100", "0", "abcd", "$$$"];
        for input_amount in invalid_input_amounts.iter().map(|i| i.to_string()) {
            let mut invalid = get_valid_params();
            invalid.input_amount = input_amount;

            assert_eq!(
                validate_params(&invalid).unwrap_err(),
                "Invalid input amount provided"
            );
        }
    }

    #[test]
    fn validates_input_return_address() {
        let mut missing_return_address = get_valid_params();
        missing_return_address.input_coin = Coin::LOKI;
        missing_return_address.input_return_address = None;

        assert_eq!(
            validate_params(&missing_return_address).unwrap_err(),
            "Input return address not provided"
        );

        let mut invalid_address = get_valid_params();
        invalid_address.input_return_address = Some("i'm an address! weeeee!".to_string());

        assert_eq!(
            validate_params(&invalid_address).unwrap_err(),
            "Invalid return address"
        );
    }

    #[test]
    fn validates_input_address_id() {
        let mut invalid = get_valid_params();
        invalid.input_address_id = "i am not invalid, i am outvalid".to_owned();

        assert_eq!(
            validate_params(&invalid).unwrap_err(),
            "Invalid input id provided"
        );
    }

    #[test]
    fn validates_output_address() {
        let mut invalid_address = get_valid_params();
        invalid_address.output_address = "i'm an address! weeeee!".to_string();

        assert_eq!(
            validate_params(&invalid_address).unwrap_err(),
            "Invalid output address"
        );
    }

    #[test]
    fn validates_slippage() {
        let invalid_values: Vec<u32> = vec![10_001, 11_000];
        for value in invalid_values.into_iter() {
            let mut params = get_valid_params();
            params.slippage_limit = value;

            assert_eq!(
                validate_params(&params).unwrap_err(),
                "Slippage limit must be between 0 and 10000"
            );
        }

        // Setting slippage requires a return address to be set

        let params = SwapQuoteParams {
            input_coin: Coin::ETH,
            input_return_address: None,
            input_address_id: "10".to_owned(),
            input_amount: "1000000000".to_string(),
            output_coin: Coin::LOKI,
            output_address: LOKI_ADDRESS.to_string(),
            slippage_limit: 10,
        };

        assert_eq!(
            validate_params(&params).unwrap_err(),
            "Input return address not provided"
        );
    }

    #[test]
    fn validates_address() {
        // Insert values to test below

        let mut map = HashMap::new();
        map.insert(Coin::LOKI, Values {
            invalid: vec![ETH_ADDRESS, "abcdefg", "T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kH"],
            valid: vec![LOKI_ADDRESS]
        });

        map.insert(
            Coin::ETH,
            Values {
                invalid: vec![
                    LOKI_ADDRESS,
                    "abcdefg",
                    "70e7db0678460c5e53f1ffc9221d1c692111d",
                ],
                valid: vec![ETH_ADDRESS, "70e7db0678460c5e53f1ffc9221d1c692111dcc5"],
            },
        );

        // Perform the test

        for (coin, values) in map {
            for invalid in values.invalid {
                assert!(
                    validate_address(coin, invalid).is_err(),
                    "Expected {} to be an invalid address for {}",
                    invalid,
                    coin
                );
            }

            for valid in values.valid {
                assert!(
                    validate_address(coin, valid).is_ok(),
                    "Expected {} to be a valid address for {}",
                    valid,
                    coin
                );
            }
        }
    }
}
