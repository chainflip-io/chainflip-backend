use crate::common::coins::CoinAmount;
use std::convert::AsMut;

/// The test utils
pub mod test_utils;

/// Loki utils
pub mod loki;

/// Utils for generating HD wallets (bip32/bip44)
pub mod bip44;

/// Utils for asymmetric swapping
pub mod autoswap;

/// Clone slice values into an array
///
/// # Example
///
/// ```
/// use blockswap::utils::clone_into_array;
///
/// let original = [1, 2, 3, 4, 5];
/// let cloned: [u8; 4] = clone_into_array(&original[..4]);
/// assert_eq!(cloned, [1, 2, 3, 4]);
/// ```
pub fn clone_into_array<A, T>(slice: &[T]) -> A
where
    A: Sized + Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
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
