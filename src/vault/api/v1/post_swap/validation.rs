use super::SwapQuoteParams;
use chainflip_common::{
    types::{fraction::PercentageFraction, Network},
    utils::address_id,
    validation::{validate_address, validate_address_id},
};

/// Validate quote params
pub fn validate_params(params: &SwapQuoteParams, network: Network) -> Result<(), &'static str> {
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
        if validate_address(params.input_coin, network, &return_address).is_err() {
            return Err("Invalid return address");
        }
    }

    if validate_address(params.output_coin, network, &params.output_address).is_err() {
        return Err("Invalid output address");
    }

    let input_address_id = address_id::to_bytes(params.input_coin, &params.input_address_id)
        .map_err(|_| "Invalid input id provided")?;
    if validate_address_id(params.input_coin, &input_address_id).is_err() {
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
    use super::*;
    use crate::utils::test_utils::{TEST_ETH_ADDRESS, TEST_ETH_SALT, TEST_LOKI_ADDRESS};
    use chainflip_common::types::coin::Coin;

    fn get_valid_params() -> SwapQuoteParams {
        SwapQuoteParams {
            input_coin: Coin::LOKI,
            input_return_address: Some(TEST_LOKI_ADDRESS.to_string()),
            input_address_id: "60900e5603bf96e3".to_owned(),
            input_amount: "1000000000".to_string(),
            output_coin: Coin::ETH,
            output_address: TEST_ETH_ADDRESS.to_string(),
            slippage_limit: 0,
        }
    }

    #[test]
    fn validates_correctly() {
        let valid = get_valid_params();
        assert_eq!(validate_params(&valid, Network::Testnet), Ok(()));
    }

    #[test]
    fn validates_coins() {
        let mut invalid = get_valid_params();
        invalid.input_coin = Coin::ETH;
        invalid.input_return_address = Some(invalid.output_address.clone());

        assert_eq!(
            validate_params(&invalid, Network::Testnet).unwrap_err(),
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
                validate_params(&invalid, Network::Testnet).unwrap_err(),
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
            validate_params(&missing_return_address, Network::Testnet).unwrap_err(),
            "Input return address not provided"
        );

        let mut invalid_address = get_valid_params();
        invalid_address.input_return_address = Some("i'm an address! weeeee!".to_string());

        assert_eq!(
            validate_params(&invalid_address, Network::Testnet).unwrap_err(),
            "Invalid return address"
        );
    }

    #[test]
    fn validates_input_address_id() {
        let mut invalid = get_valid_params();
        invalid.input_address_id = "i am not invalid, i am outvalid".to_owned();

        assert_eq!(
            validate_params(&invalid, Network::Testnet).unwrap_err(),
            "Invalid input id provided"
        );
    }

    #[test]
    fn validates_output_address() {
        let mut invalid_address = get_valid_params();
        invalid_address.output_address = "i'm an address! weeeee!".to_string();

        assert_eq!(
            validate_params(&invalid_address, Network::Testnet).unwrap_err(),
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
                validate_params(&params, Network::Testnet).unwrap_err(),
                "Slippage limit must be between 0 and 10000"
            );
        }

        // Setting slippage requires a return address to be set

        let params = SwapQuoteParams {
            input_coin: Coin::ETH,
            input_return_address: None,
            input_address_id: hex::encode(TEST_ETH_SALT),
            input_amount: "1000000000".to_string(),
            output_coin: Coin::LOKI,
            output_address: TEST_LOKI_ADDRESS.to_string(),
            slippage_limit: 10,
        };

        assert_eq!(
            validate_params(&params, Network::Testnet).unwrap_err(),
            "Input return address not provided"
        );
    }
}
