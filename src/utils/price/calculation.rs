use crate::common::Coin;
use crate::utils::primitives::U512;
use std::convert::TryFrom;

/// A structure for representing a normalised amount
///
/// The calculation below requires that all the atomic amounts use the same decimal places or it will return incorrect values.
/// This struct is used to convert all atomic amounts to the same decimal place values.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct NormalisedAmount(U512);

impl NormalisedAmount {
    /// The amount of decimals to normalise.
    ///
    /// **This should always be greater than the largest decimal for a coin**
    const DECIMALS: u32 = 18;

    /// Create a normalised amount using the given atomic amount and coin
    pub fn from(amount: u128, coin: Coin) -> Self {
        let decimals = coin.get_info().decimals;
        if decimals > Self::DECIMALS {
            error!(
                "{}({}) has more decimals than normalise amount({}) supports!",
                coin,
                decimals,
                Self::DECIMALS
            );
            panic!("Larger decimal was used");
        }

        let decimals = Self::DECIMALS.saturating_sub(decimals);
        let ten_pow = U512::from(10)
            .checked_pow(decimals.into())
            .expect("Overflow occurred when calculating normalised amount");
        let amount = U512::from(amount)
            .checked_mul(ten_pow)
            .expect("Overflow occurred when calculating normalised amount");
        NormalisedAmount(amount)
    }

    /// Convert a normalised amount back into a coin atomic amount
    /// Return `None` if overflow occurs
    pub fn to_atomic(&self, coin: Coin) -> Option<u128> {
        let decimals = coin.get_info().decimals;
        if decimals > Self::DECIMALS {
            error!(
                "{}({}) has more decimals than normalise amount({}) supports!",
                coin,
                decimals,
                Self::DECIMALS
            );
            panic!("Larger decimal was used");
        }

        let decimals = Self::DECIMALS.saturating_sub(decimals);
        let ten_pow = U512::from(10)
            .checked_pow(decimals.into())
            .expect("Overflow occurred when converting normalised amount");
        let amount = self.0 / ten_pow;
        u128::try_from(amount).ok()
    }
}

/// Calculate the output amount
fn calculate_output_amount_normalised(
    input_amount: NormalisedAmount,
    input_depth: NormalisedAmount,
    input_fee: NormalisedAmount,
    output_depth: NormalisedAmount,
    output_fee: NormalisedAmount,
) -> NormalisedAmount {
    let zero = NormalisedAmount(0.into());

    let input_amount = input_amount.0;
    let input_fee = input_fee.0;
    let input_depth = input_depth.0;
    let output_depth = output_depth.0;
    let output_fee = output_fee.0;

    let numerator = input_amount.saturating_sub(input_fee);

    // Check for overflows
    let numerator = match numerator.checked_mul(input_depth) {
        Some(value) => value,
        None => return zero,
    };

    let numerator = match numerator.checked_mul(output_depth) {
        Some(value) => value,
        None => return zero,
    };

    let denominator = match input_amount.checked_add(input_depth) {
        Some(value) => value,
        None => return zero,
    };

    let denominator = match denominator.checked_pow(2.into()) {
        Some(value) => value,
        None => return zero,
    };

    if denominator == 0.into() {
        return zero;
    }

    let output_amount: U512 = (numerator / denominator).saturating_sub(output_fee);
    NormalisedAmount(output_amount)
}

pub(crate) fn calculate_output_amount(
    input_coin: Coin,
    input_amount: u128,
    input_depth: u128,
    input_fee: u128,
    output_coin: Coin,
    output_depth: u128,
    output_fee: u128,
) -> Option<u128> {
    let output_amount = calculate_output_amount_normalised(
        NormalisedAmount::from(input_amount, input_coin),
        NormalisedAmount::from(input_depth, input_coin),
        NormalisedAmount::from(input_fee, input_coin),
        NormalisedAmount::from(output_depth, output_coin),
        NormalisedAmount::from(output_fee, output_coin),
    );

    output_amount.to_atomic(output_coin)
}

#[cfg(test)]
mod test {
    use crate::common::LokiAmount;

    use super::*;

    fn normalise_loki_decimal(amount: f64) -> NormalisedAmount {
        let atomic = LokiAmount::from_decimal_string(&amount.to_string()).to_atomic();
        NormalisedAmount::from(atomic, Coin::LOKI)
    }

    #[test]
    fn normalised_amount_correctly_normalises() {
        let eth_normalised = NormalisedAmount::from(1, Coin::ETH);
        let loki_normalised = NormalisedAmount::from(1, Coin::LOKI);

        assert_eq!(eth_normalised, NormalisedAmount(1.into()));
        assert_eq!(loki_normalised, NormalisedAmount(1000000000u128.into()));
        assert_eq!(loki_normalised.to_atomic(Coin::LOKI), Some(1));
    }

    #[test]
    fn normalised_amount_handles_max_u128() {
        let normalised = NormalisedAmount::from(u128::MAX, Coin::ETH);
        assert_eq!(normalised, NormalisedAmount(u128::MAX.into()));
    }

    #[test]
    fn calculates_correct_output_amount() {
        let values = vec![
            // No fees
            (1000.0, 10000.0, 0.0, 20000.0, 0.0, 1652.892561983471),
            // Fees
            (1000.0, 10000.0, 0.5, 20000.0, 0.0, 1652.0661157024792),
            (1000.0, 10000.0, 0.0, 20000.0, 0.5, 1652.392561983471),
            // Fees greater than input and output
            (1000.0, 10000.0, 20000.0, 20000.0, 0.0, 0.0),
            (1000.0, 10000.0, 0.0, 20000.0, 20000.0, 0.0),
            // Invalid values
            (0.0, 1.0, 0.0, 2.0, 0.0, 0.0),
            (1.0, 0.0, 0.0, 2.0, 0.0, 0.0),
            (1.0, 1.0, 0.0, 0.0, 0.0, 0.0),
            (1000.0, 10000.0, 0.0, 20000.0, 1000000000.0, 0.0),
        ];

        for value in values.iter() {
            let output = calculate_output_amount_normalised(
                normalise_loki_decimal(value.0),
                normalise_loki_decimal(value.1),
                normalise_loki_decimal(value.2),
                normalise_loki_decimal(value.3),
                normalise_loki_decimal(value.4),
            );
            assert_eq!(
                output.to_atomic(Coin::LOKI),
                Some(LokiAmount::from_decimal_string(&value.5.to_string()).to_atomic()),
            );
        }
    }

    #[test]
    fn calculates_output_amount_handles_overflow() {
        let output = calculate_output_amount_normalised(
            NormalisedAmount(u128::MAX.into()),
            NormalisedAmount(u128::MAX.into()),
            NormalisedAmount(0.into()),
            NormalisedAmount(u128::MAX.into()),
            NormalisedAmount(0.into()),
        );
        assert!(output.0 > 0.into());
    }
}
