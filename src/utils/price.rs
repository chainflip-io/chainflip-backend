use crate::{
    common::{
        coins::{CoinAmount, GenericCoinAmount, PoolCoin},
        Coin, LokiAmount,
    },
    vault::transactions::{Liquidity, TransactionProvider},
};

/// Input, Output, Fee
pub type Output = (u128, u128, u128);

/// The loki fee
pub const LOKI_FEE_DECIMAL: f64 = 0.5;

// Note: Ugly code below :(, haven't thought of a good way to handle this yet

/// Get the output amount.
///
/// The tuples returned are in the format `(input, output, fee)`.
///
/// If `input` or `output` is `LOKI` then only 1 tuple is returned.
///
/// If `input` or `output` is *NOT* `LOKI` then 2 tuples are returned: `[(input, LOKI, fee), (LOKI, output, fee)]`
pub fn get_output_amount<F>(
    input: Coin,
    input_amount: u128,
    output: Coin,
    get_liquidity: F,
) -> Result<Vec<Output>, &'static str>
where
    F: FnOnce(PoolCoin) -> Option<Liquidity> + Copy,
{
    if input == output {
        return Err("Cannot get output amount for the same coin");
    }

    if input == Coin::LOKI || output == Coin::LOKI {
        get_output_amount_inner(input, input_amount, output, LOKI_FEE_DECIMAL, get_liquidity)
            .map(|result| vec![result])
    } else {
        let first = get_output_amount_inner(
            input,
            input_amount,
            Coin::LOKI,
            LOKI_FEE_DECIMAL,
            get_liquidity,
        )?;
        let second = get_output_amount_inner(Coin::LOKI, first.1, output, 0.0, get_liquidity)?;
        Ok(vec![first, second])
    }
}

// Inner calculation
fn get_output_amount_inner<F>(
    input: Coin,
    input_amount: u128,
    output: Coin,
    loki_fee: f64,
    get_liquidity: F,
) -> Result<Output, &'static str>
where
    F: FnOnce(PoolCoin) -> Option<Liquidity>,
{
    if input == output {
        return Err("Cannot get output amount for the same coin");
    }

    let loki_fee = LokiAmount::from_decimal(loki_fee);

    if input == Coin::LOKI {
        let pool_coin = PoolCoin::from(output).expect("Expected output to be a valid pool coin");
        let liquidity = get_liquidity(pool_coin).unwrap_or(Liquidity::new());

        let loki_decimal = LokiAmount::from_atomic(input_amount).to_decimal();
        let loki_depth = LokiAmount::from_atomic(liquidity.loki_depth).to_decimal();
        let output_depth = GenericCoinAmount::atmoic(output, liquidity.depth).to_decimal();
        let output_amount = calculate_output_amount(
            loki_decimal,
            loki_depth,
            loki_fee.to_decimal(),
            output_depth,
            0.0,
        );
        let output_amount = GenericCoinAmount::decimal(output, output_amount).to_atomic();

        Ok((input_amount, output_amount, loki_fee.to_atomic()))
    } else if output == Coin::LOKI {
        let pool_coin = PoolCoin::from(input).expect("Expected input to be a valid pool coin");
        let liquidity = get_liquidity(pool_coin).unwrap_or(Liquidity::new());

        let input_decimal = GenericCoinAmount::atmoic(input, input_amount).to_decimal();
        let input_depth = GenericCoinAmount::atmoic(input, liquidity.depth).to_decimal();
        let loki_depth = LokiAmount::from_atomic(liquidity.loki_depth).to_decimal();

        let output_amount = calculate_output_amount(
            input_decimal,
            input_depth,
            0.0,
            loki_depth,
            loki_fee.to_decimal(),
        );

        let output_amount = LokiAmount::from_decimal(output_amount).to_atomic();

        Ok((input_amount, output_amount, loki_fee.to_atomic()))
    } else {
        Err("LOKI coin needs to be passed into either input or output")
    }
}

/// Calculate the output amount in decimals from the given input amount, input and output depths and fees
pub fn calculate_output_amount(
    input_amount: f64,
    input_depth: f64,
    input_fee: f64,
    output_depth: f64,
    output_fee: f64,
) -> f64 {
    if input_amount <= 0.0 || input_depth <= 0.0 || output_depth <= 0.0 {
        return 0.0;
    }

    let input_fee = input_fee.max(0.0);
    let output_fee = output_fee.max(0.0);

    let output_amount = (input_amount - input_fee) * input_depth * output_depth
        / (input_amount + input_depth).powi(2);

    (output_amount - output_fee).max(0.0)
}

#[cfg(test)]
mod test {
    use super::*;

    struct CalculateOutputValues {
        pub input_amount: f64,
        pub input_depth: f64,
        pub input_fee: f64,
        pub output_depth: f64,
        pub output_fee: f64,
        pub output_amount: f64,
    }

    impl CalculateOutputValues {
        /// Shorthand for creating a value
        pub fn new(
            input_amount: f64,
            input_depth: f64,
            input_fee: f64,
            output_depth: f64,
            output_fee: f64,
            output_amount: f64,
        ) -> Self {
            CalculateOutputValues {
                input_amount,
                input_depth,
                input_fee,
                output_depth,
                output_fee,
                output_amount,
            }
        }
    }

    #[test]
    fn calculates_correct_output_amount() {
        let values = vec![
            // No fees
            CalculateOutputValues::new(1000.0, 10000.0, 0.0, 20000.0, 0.0, 1652.892561983471),
            CalculateOutputValues::new(1000.0, 10000.0, -0.1, 20000.0, 0.0, 1652.892561983471),
            CalculateOutputValues::new(1000.0, 10000.0, 0.0, 20000.0, -0.1, 1652.892561983471),
            // Fees
            CalculateOutputValues::new(1000.0, 10000.0, 0.5, 20000.0, 0.0, 1652.0661157024792),
            CalculateOutputValues::new(1000.0, 10000.0, 0.0, 20000.0, 0.5, 1652.392561983471),
            // Invalid values
            CalculateOutputValues::new(0.0, 1.0, 0.0, 2.0, 0.0, 0.0),
            CalculateOutputValues::new(1.0, 0.0, 0.0, 2.0, 0.0, 0.0),
            CalculateOutputValues::new(1.0, 1.0, 0.0, 0.0, 0.0, 0.0),
            CalculateOutputValues::new(1000.0, 10000.0, 0.0, 20000.0, 1000000000.0, 0.0),
        ];

        for value in values.iter() {
            assert_eq!(
                calculate_output_amount(
                    value.input_amount,
                    value.input_depth,
                    value.input_fee,
                    value.output_depth,
                    value.output_fee
                ),
                value.output_amount
            );
        }
    }
}
