use crate::common::Coin;
use crate::utils::primitives::U512;
use std::convert::TryFrom;

/// A structure for representing a normalised amount
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct NormalisedAmount(u128);

impl NormalisedAmount {
    /// The amount of decimals to normalise
    const DECIMALS: u32 = 18;

    /// Create a normalised amount using the given atomic amount and coin
    pub fn from(amount: u128, coin: Coin) -> Self {
        let decimals = coin.get_info().decimals;
        let decimals = Self::DECIMALS.saturating_sub(decimals);
        let ten_pow = 10u128.saturating_pow(decimals);
        NormalisedAmount(amount.saturating_mul(ten_pow))
    }

    /// Convert a normalised amount back into a coin atomic amount
    pub fn to_atomic(&self, coin: Coin) -> u128 {
        let decimals = coin.get_info().decimals;
        let decimals = Self::DECIMALS.saturating_sub(decimals);
        let ten_pow = 10u128.saturating_pow(decimals);
        self.0 / ten_pow
    }
}

/// Calculate the output amount
pub fn calculate_output_amount(
    input_amount: NormalisedAmount,
    input_depth: NormalisedAmount,
    input_fee: NormalisedAmount,
    output_depth: NormalisedAmount,
    output_fee: NormalisedAmount,
) -> NormalisedAmount {
    let input_amount = U512::from(input_amount.0);
    let input_fee = U512::from(input_fee.0);
    let input_depth = U512::from(input_depth.0);
    let output_depth = U512::from(output_depth.0);
    let output_fee = U512::from(output_fee.0);

    let numerator = input_amount
        .saturating_sub(input_fee)
        .saturating_mul(input_depth)
        .saturating_mul(output_depth);

    let denominator = input_amount
        .saturating_add(input_depth)
        .checked_pow(U512::from(2));

    // check overflow
    let denominator = match denominator {
        Some(value) => value,
        None => return NormalisedAmount(0),
    };

    let output_amount: U512 = (numerator / denominator).saturating_sub(output_fee);
    let output_amount = u128::try_from(output_amount).unwrap_or(0);
    NormalisedAmount(output_amount)
}

/// Calculate the output amount in decimals from the given input amount, input and output depths and fees
pub fn calculate_output_amount_deprecated(
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
    use crate::common::LokiAmount;

    use super::*;

    #[test]
    fn correctly_normalises() {
        let eth_normalised = NormalisedAmount::from(1, Coin::ETH);
        let loki_normalised = NormalisedAmount::from(1, Coin::LOKI);

        assert_eq!(eth_normalised, NormalisedAmount(1));
        assert_eq!(loki_normalised, NormalisedAmount(1000000000));
        assert_eq!(loki_normalised.to_atomic(Coin::LOKI), 1);
    }

    fn normalise_loki_decimal(amount: f64) -> NormalisedAmount {
        let atomic = LokiAmount::from_decimal(amount).to_atomic();
        NormalisedAmount::from(atomic, Coin::LOKI)
    }

    #[test]
    fn calculates_correct_output_amount() {
        let values = vec![
            // No fees
            (1000.0, 10000.0, 0.0, 20000.0, 0.0, 1652.892561983471),
            // Fees
            (1000.0, 10000.0, 0.5, 20000.0, 0.0, 1652.0661157024792),
            (1000.0, 10000.0, 0.0, 20000.0, 0.5, 1652.392561983471),
            // Invalid values
            (0.0, 1.0, 0.0, 2.0, 0.0, 0.0),
            (1.0, 0.0, 0.0, 2.0, 0.0, 0.0),
            (1.0, 1.0, 0.0, 0.0, 0.0, 0.0),
            (1000.0, 10000.0, 0.0, 20000.0, 1000000000.0, 0.0),
        ];

        for value in values.iter() {
            let output = calculate_output_amount(
                normalise_loki_decimal(value.0),
                normalise_loki_decimal(value.1),
                normalise_loki_decimal(value.2),
                normalise_loki_decimal(value.3),
                normalise_loki_decimal(value.4),
            );
            assert_eq!(
                output.to_atomic(Coin::LOKI),
                LokiAmount::from_decimal(value.5).to_atomic(),
            );
        }
    }

    #[test]
    fn calculates_output_amount_handles_overflow() {
        let output = calculate_output_amount(
            NormalisedAmount(u128::MAX),
            NormalisedAmount(u128::MAX),
            NormalisedAmount(0),
            NormalisedAmount(u128::MAX),
            NormalisedAmount(0),
        );
        assert!(output.0 > 0);
    }
}
