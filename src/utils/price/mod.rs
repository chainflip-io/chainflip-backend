use crate::{
    common::liquidity_provider::{Liquidity, LiquidityProvider},
    common::{coins::PoolCoin, OxenAmount},
    constants::OXEN_SWAP_PROCESS_FEE,
};
pub use calculation::*;
use chainflip_common::types::{
    chain::{PoolChange, Validate},
    coin::Coin,
    Network,
};
use std::convert::TryFrom;

mod calculation;

/// Details about an output
#[derive(Debug, Copy, Clone)]
pub struct OutputDetail {
    /// The input coin
    pub input: Coin,
    /// The input amount
    pub input_amount: u128,
    /// The output coin
    pub output: Coin,
    /// The output amount
    pub output_amount: u128,
    /// The fee paid in oxen
    pub oxen_fee: u128,
}

impl OutputDetail {
    /// Convert this output detail to a pool change transaction
    pub fn to_pool_change_tx(&self) -> Result<PoolChange, &'static str> {
        if self.input != Coin::OXEN && self.output != Coin::OXEN {
            return Err("Cannot make a PoolChangeTx without a OXEN input or output");
        }

        let input_depth = i128::try_from(self.input_amount)
            .map_err(|_| "Failed to convert input depth to i128")?;
        let output_depth = i128::try_from(self.output_amount)
            .map_err(|_| "Failed to convert output depth to i128")?;
        let output_depth = -1 * output_depth;

        let is_input_base = self.input == Coin::BASE_COIN;
        let pool_coin = if is_input_base {
            self.output
        } else {
            self.input
        };

        let depth_change = if is_input_base {
            output_depth
        } else {
            input_depth
        };
        let base_depth_change = if is_input_base {
            input_depth
        } else {
            output_depth
        };

        let change = PoolChange::new(pool_coin, depth_change, base_depth_change, None);

        // Network type shouldn't really matter here
        // Just validatinf to ensure base and depth change are correct
        change.validate(Network::Mainnet)?;

        Ok(change)
    }
}

/// The Output calculation.
///
/// Always has the property: `first.output == second.input`
#[derive(Debug)]
pub struct OutputCalculation {
    /// The first calculation
    pub first: OutputDetail,
    /// The second calculation
    pub second: Option<OutputDetail>,
}

impl OutputCalculation {
    /// Create a new output calculation
    pub fn new(first: OutputDetail, second: Option<OutputDetail>) -> Self {
        if let Some(second) = &second {
            if first.output != second.input {
                panic!("First output doesn't match second input")
            }
        }

        OutputCalculation { first, second }
    }
}

// Note: Ugly code below :(, haven't thought of a good way to handle this yet

/// Get the output amount.
///
/// If `input` or `output` is *NOT* `OXEN` then `first` will contain `input -> OXEN` and `second` will contain `OXEN -> output`
pub fn get_output<T: LiquidityProvider>(
    provider: &T,
    input: Coin,
    input_amount: u128,
    output: Coin,
) -> Result<OutputCalculation, &'static str> {
    if input == output {
        return Err("Cannot get output amount for the same coin");
    }

    let fee = OxenAmount::from_atomic(OXEN_SWAP_PROCESS_FEE);

    if input == Coin::OXEN || output == Coin::OXEN {
        get_output_amount_inner(provider, input, input_amount, output, fee)
            .map(|result| OutputCalculation::new(result, None))
    } else {
        let first = get_output_amount_inner(provider, input, input_amount, Coin::OXEN, fee)?;

        let second = get_output_amount_inner(
            provider,
            Coin::OXEN,
            first.output_amount,
            output,
            OxenAmount::from_atomic(0),
        )?;
        Ok(OutputCalculation::new(first, Some(second)))
    }
}

// Inner calculation
fn get_output_amount_inner<T: LiquidityProvider>(
    provider: &T,
    input: Coin,
    input_amount: u128,
    output: Coin,
    oxen_fee: OxenAmount,
) -> Result<OutputDetail, &'static str> {
    if input == output {
        return Err("Cannot get output amount for the same coin");
    }

    if input != Coin::OXEN && output != Coin::OXEN {
        return Err("OXEN coin needs to be passed into either input or output");
    }

    let is_oxen_input = input == Coin::OXEN;

    let pool_coin = if is_oxen_input { output } else { input };
    let pool_coin =
        PoolCoin::from(pool_coin).map_err(|_| "Expected a valid pool coin to be present")?;

    let liquidity = provider
        .get_liquidity(pool_coin)
        .unwrap_or(Liquidity::zero());

    let input_depth = if is_oxen_input {
        liquidity.base_depth
    } else {
        liquidity.depth
    };

    let output_depth = if is_oxen_input {
        liquidity.depth
    } else {
        liquidity.base_depth
    };

    let input_fee = if is_oxen_input {
        oxen_fee.to_atomic()
    } else {
        0
    };

    let output_fee = if is_oxen_input {
        0
    } else {
        oxen_fee.to_atomic()
    };

    let output_amount = calculate_output_amount(
        input,
        input_amount,
        input_depth,
        input_fee,
        output,
        output_depth,
        output_fee,
    )
    .unwrap_or(0);

    Ok(OutputDetail {
        input,
        input_amount,
        output,
        output_amount,
        oxen_fee: oxen_fee.to_atomic(),
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common::{liquidity_provider::MemoryLiquidityProvider, GenericCoinAmount};

    fn to_atomic(coin: Coin, amount: &str) -> u128 {
        GenericCoinAmount::from_decimal_string(coin, amount).to_atomic()
    }

    #[test]
    fn get_output_with_oxen_input() {
        let mut provider = MemoryLiquidityProvider::new();
        provider.set_liquidity(
            PoolCoin::ETH,
            Some(Liquidity::new(
                to_atomic(Coin::ETH, "20000.0"),
                to_atomic(Coin::OXEN, "10000.0"),
            )),
        );

        let input = Coin::OXEN;
        let input_amount = to_atomic(input, "2500.0");
        let output = Coin::ETH;
        let expected_output_amount = to_atomic(output, "3199.36");

        let calculation = get_output(&provider, input, input_amount, output)
            .expect("Expected to get the correct output");

        assert!(calculation.second.is_none(), "Expected only one output");

        let detail = calculation.first;
        assert_eq!(detail.input, input);
        assert_eq!(detail.input_amount, input_amount);
        assert_eq!(detail.output, output);
        assert_eq!(detail.output_amount, expected_output_amount);
        assert_eq!(detail.oxen_fee, OXEN_SWAP_PROCESS_FEE);
    }

    #[test]
    fn get_output_with_oxen_output() {
        let mut provider = MemoryLiquidityProvider::new();
        provider.set_liquidity(
            PoolCoin::ETH,
            Some(Liquidity::new(
                to_atomic(Coin::ETH, "10000.0"),
                to_atomic(Coin::OXEN, "20000.0"),
            )),
        );

        let input = Coin::ETH;
        let input_amount = to_atomic(input, "2500.0");
        let output = Coin::OXEN;
        let expected_output_amount = to_atomic(output, "3199.5");

        let calculation = get_output(&provider, input, input_amount, output)
            .expect("Expected to get the correct output");

        assert!(calculation.second.is_none(), "Expected only one output");

        let detail = calculation.first;
        assert_eq!(detail.input, input);
        assert_eq!(detail.input_amount, input_amount);
        assert_eq!(detail.output, output);
        assert_eq!(detail.output_amount, expected_output_amount);
        assert_eq!(detail.oxen_fee, OXEN_SWAP_PROCESS_FEE);
    }

    #[test]
    fn get_output_with_non_oxen_input_output() {
        let mut provider = MemoryLiquidityProvider::new();
        provider.set_liquidity(
            PoolCoin::ETH,
            Some(Liquidity::new(
                to_atomic(Coin::ETH, "10000.0"),
                to_atomic(Coin::OXEN, "20000.0"),
            )),
        );

        provider.set_liquidity(
            PoolCoin::BTC,
            Some(Liquidity::new(
                to_atomic(Coin::BTC, "12769.0"),
                to_atomic(Coin::OXEN, "10191.0"),
            )),
        );

        let input = Coin::ETH;
        let input_amount = to_atomic(input, "2500.0");
        let output = Coin::BTC;
        let expected_output_amount = to_atomic(output, "2322.0");

        let calculation = get_output(&provider, input, input_amount, output)
            .expect("Expected to get the correct output");

        let first = calculation.first;
        assert_eq!(first.input, input);
        assert_eq!(first.input_amount, input_amount);
        assert_eq!(first.output, Coin::OXEN);
        assert_eq!(first.output_amount, to_atomic(Coin::OXEN, "3199.5"));
        assert_eq!(first.oxen_fee, OXEN_SWAP_PROCESS_FEE);

        let second = calculation.second.expect("Expected a second output");
        assert_eq!(second.input, Coin::OXEN);
        assert_eq!(second.input_amount, to_atomic(Coin::OXEN, "3199.5"));
        assert_eq!(second.output, output);
        assert_eq!(second.output_amount, expected_output_amount);
        assert_eq!(second.oxen_fee, 0);
    }

    #[test]
    fn output_detail_to_pool_change_tx() {
        // Invalid
        let invalid = OutputDetail {
            input: Coin::BTC,
            input_amount: 1,
            output: Coin::ETH,
            output_amount: 2,
            oxen_fee: 0,
        };

        assert_eq!(
            invalid.to_pool_change_tx().unwrap_err(),
            "Cannot make a PoolChangeTx without a OXEN input or output"
        );

        // Oxen input
        let oxen_input = OutputDetail {
            input: Coin::OXEN,
            input_amount: 10,
            output: Coin::ETH,
            output_amount: 20,
            oxen_fee: 0,
        };

        let pool_change = oxen_input
            .to_pool_change_tx()
            .expect("Expected a valid pool change transaction");

        assert_eq!(pool_change.pool, Coin::ETH);
        assert_eq!(pool_change.base_depth_change, 10);
        assert_eq!(pool_change.depth_change, -20);

        // Oxen output
        let oxen_output = OutputDetail {
            input: Coin::ETH,
            input_amount: 10,
            output: Coin::OXEN,
            output_amount: 20,
            oxen_fee: 0,
        };

        let pool_change = oxen_output
            .to_pool_change_tx()
            .expect("Expected a valid pool change transaction");

        assert_eq!(pool_change.pool, Coin::ETH);
        assert_eq!(pool_change.depth_change, 10);
        assert_eq!(pool_change.base_depth_change, -20);

        // Bounds
        let max_input = OutputDetail {
            input: Coin::OXEN,
            input_amount: u128::MAX,
            output: Coin::ETH,
            output_amount: 20,
            oxen_fee: 0,
        };

        assert_eq!(
            max_input.to_pool_change_tx().unwrap_err(),
            "Failed to convert input depth to i128"
        );

        let max_output = OutputDetail {
            input: Coin::OXEN,
            input_amount: 20,
            output: Coin::ETH,
            output_amount: u128::MAX,
            oxen_fee: 0,
        };

        assert_eq!(
            max_output.to_pool_change_tx().unwrap_err(),
            "Failed to convert output depth to i128"
        );
    }
}
