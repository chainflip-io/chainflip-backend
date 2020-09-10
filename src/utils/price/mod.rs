use crate::{
    common::loki_process_fee,
    common::{
        coins::{CoinAmount, GenericCoinAmount, PoolCoin},
        Coin, LokiAmount,
    },
    transactions::PoolChangeTx,
    vault::transactions::LiquidityProvider,
    vault::transactions::{Liquidity, TransactionProvider},
};
use std::convert::TryFrom;

mod calculation;
pub use calculation::*;

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
    /// The fee paid in loki
    pub loki_fee: u128,
}

impl OutputDetail {
    /// Convert this output detail to a pool change transaction
    pub fn to_pool_change_tx(&self) -> Result<PoolChangeTx, &'static str> {
        if self.input != Coin::LOKI && self.output != Coin::LOKI {
            return Err("Cannot make a PoolChangeTx without a LOKI input or output");
        }

        let input_depth = i128::try_from(self.input_amount)
            .map_err(|_| "Failed to convert input depth to i128")?;
        let output_depth = i128::try_from(self.output_amount)
            .map_err(|_| "Failed to convert output depth to i128")?;
        let output_depth = -1 * output_depth;

        let is_input_loki = self.input == Coin::LOKI;
        let pool_coin = if is_input_loki {
            self.output
        } else {
            self.input
        };
        let pool_coin = PoolCoin::from(pool_coin)?;

        let depth_change = if is_input_loki {
            output_depth
        } else {
            input_depth
        };
        let loki_depth_change = if is_input_loki {
            input_depth
        } else {
            output_depth
        };

        Ok(PoolChangeTx::new(
            pool_coin,
            loki_depth_change,
            depth_change,
        ))
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
/// If `input` or `output` is *NOT* `LOKI` then `first` will contain `input -> LOKI` and `second` will contain `LOKI -> output`
pub fn get_output<T: LiquidityProvider>(
    provider: &T,
    input: Coin,
    input_amount: u128,
    output: Coin,
) -> Result<OutputCalculation, &'static str> {
    if input == output {
        return Err("Cannot get output amount for the same coin");
    }

    let fee = loki_process_fee();

    if input == Coin::LOKI || output == Coin::LOKI {
        get_output_amount_inner(provider, input, input_amount, output, fee)
            .map(|result| OutputCalculation::new(result, None))
    } else {
        let first = get_output_amount_inner(provider, input, input_amount, Coin::LOKI, fee)?;

        let second = get_output_amount_inner(
            provider,
            Coin::LOKI,
            first.output_amount,
            output,
            LokiAmount::from_atomic(0),
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
    loki_fee: LokiAmount,
) -> Result<OutputDetail, &'static str> {
    if input == output {
        return Err("Cannot get output amount for the same coin");
    }

    if input != Coin::LOKI && output != Coin::LOKI {
        return Err("LOKI coin needs to be passed into either input or output");
    }

    let is_loki_input = input == Coin::LOKI;

    let pool_coin = if is_loki_input { output } else { input };
    let pool_coin =
        PoolCoin::from(pool_coin).map_err(|_| "Expected a valid pool coin to be present")?;

    let liquidity = provider
        .get_liquidity(pool_coin)
        .unwrap_or(Liquidity::new());

    let input_depth = if is_loki_input {
        liquidity.loki_depth
    } else {
        liquidity.depth
    };

    let output_depth = if is_loki_input {
        liquidity.depth
    } else {
        liquidity.loki_depth
    };

    let input_fee = if is_loki_input {
        loki_fee.to_atomic()
    } else {
        0
    };

    let output_fee = if is_loki_input {
        0
    } else {
        loki_fee.to_atomic()
    };

    let output_amount = calculate_output_amount(
        NormalisedAmount::from(input_amount, input),
        NormalisedAmount::from(input_depth, input),
        NormalisedAmount::from(input_fee, input),
        NormalisedAmount::from(output_depth, output),
        NormalisedAmount::from(output_fee, output),
    );

    let output_amount = output_amount.to_atomic(output).unwrap_or(0);

    Ok(OutputDetail {
        input,
        input_amount,
        output,
        output_amount,
        loki_fee: loki_fee.to_atomic(),
    })
}
