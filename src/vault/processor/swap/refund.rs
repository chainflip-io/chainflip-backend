use crate::{
    transactions::QuoteTx, vault::processor::utils::is_swap_quote_expired,
    vault::transactions::memory_provider::FulfilledTxWrapper,
};

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum Result {
    NoReturnAddress,
    QuoteExpired,
    QuoteFulfilled,
    InvalidSlippageLimit,
    ZeroOutputAmount,
    SlippageLimitExceeded,
    SlippageValid,
}

impl Result {
    // Wether we should refund
    fn should_refund(&self) -> bool {
        match self {
            Self::NoReturnAddress => false,
            Self::QuoteExpired => true,
            Self::QuoteFulfilled => true,
            Self::InvalidSlippageLimit => false,
            Self::ZeroOutputAmount => true,
            Self::SlippageLimitExceeded => true,
            Self::SlippageValid => false,
        }
    }
}

fn check_refund(
    quote: &FulfilledTxWrapper<QuoteTx>,
    input_amount: u128,
    output_amount: u128,
) -> Result {
    if output_amount == 0 {
        return Result::ZeroOutputAmount;
    }

    if quote.inner.return_address.is_none() {
        return Result::NoReturnAddress;
    }

    if quote.fulfilled {
        return Result::QuoteFulfilled;
    }

    if is_swap_quote_expired(&quote.inner) {
        return Result::QuoteExpired;
    }

    // Slippage limit of 0 means we swap regardless of the limit
    let slippage_limit = quote.inner.slippage_limit;
    if slippage_limit <= 0.0 {
        return Result::InvalidSlippageLimit;
    }

    let effective_price = input_amount as f64 / output_amount as f64;

    // Calculate the slippage.
    // This will return negative value if we get a better price than what was quoted.
    let slippage = 1.0 - (quote.inner.effective_price / effective_price);

    if slippage > slippage_limit as f64 {
        Result::SlippageLimitExceeded
    } else {
        Result::SlippageValid
    }
}

/// Check wether we should refund the user.
///
/// We should refund the user if we have a refund address **AND**:
///     - Quote was already fulfilled
///     - Quote has expired
///     - Output amount is zero
///     - Slippage limit is above 0.0
///     - Slippage between quote effective price and current effective price is greater than the slippage limit
pub fn should_refund(
    quote: &FulfilledTxWrapper<QuoteTx>,
    input_amount: u128,
    output_amount: u128,
) -> bool {
    check_refund(&quote, input_amount, output_amount).should_refund()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{common::Coin, common::Timestamp, common::WalletAddress};

    fn get_quote() -> FulfilledTxWrapper<QuoteTx> {
        let quote = QuoteTx::new(
            Timestamp::now(),
            Coin::ETH,
            WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5"),
            "6".to_owned(),
            Some(WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5")),
            Coin::LOKI,
            WalletAddress::new("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY"),
            1.0,
            0.1,
        ).unwrap();

        FulfilledTxWrapper {
            inner: quote,
            fulfilled: false,
        }
    }

    #[test]
    fn test_check_refund() {
        // No return address
        let mut quote = get_quote();
        quote.inner.return_address = None;
        assert_eq!(check_refund(&quote, 100, 100), Result::NoReturnAddress);

        // Quote fulfilled
        let mut quote = get_quote();
        quote.fulfilled = true;

        assert_eq!(check_refund(&quote, 100, 100), Result::QuoteFulfilled);

        // Quote expired
        let mut quote = get_quote();
        quote.inner.timestamp = Timestamp(0);

        assert_eq!(check_refund(&quote, 100, 100), Result::QuoteExpired);

        // No slippage set
        let mut quote = get_quote();
        let values = vec![0.0, -1.0];
        for value in values {
            quote.inner.slippage_limit = value;
            assert_eq!(check_refund(&quote, 100, 100), Result::InvalidSlippageLimit);
        }

        // Zero output amount
        let quote = get_quote();
        assert_eq!(check_refund(&quote, 100, 0), Result::ZeroOutputAmount);

        // Received more coins than quoted
        let mut quote = get_quote();
        quote.inner.effective_price = 1.0; // 1:1 ratio
        quote.inner.slippage_limit = 0.1;

        assert_eq!(check_refund(&quote, 100, 130), Result::SlippageValid);

        // Slippage not exceeded
        let mut quote = get_quote();
        quote.inner.effective_price = 1.0; // 1:1 ratio
        quote.inner.slippage_limit = 0.2;

        assert_eq!(check_refund(&quote, 100, 80), Result::SlippageValid);

        // Slippage exceeded
        let mut quote = get_quote();
        quote.inner.effective_price = 1.0; // 1:1 ratio
        quote.inner.slippage_limit = 0.2;

        assert_eq!(check_refund(&quote, 100, 79), Result::SlippageLimitExceeded);
    }

    #[test]
    fn test_result_should_refund() {
        assert_eq!(Result::NoReturnAddress.should_refund(), false);
        assert_eq!(Result::QuoteExpired.should_refund(), true);
        assert_eq!(Result::QuoteFulfilled.should_refund(), true);
        assert_eq!(Result::InvalidSlippageLimit.should_refund(), false);
        assert_eq!(Result::ZeroOutputAmount.should_refund(), true);
        assert_eq!(Result::SlippageLimitExceeded.should_refund(), true);
        assert_eq!(Result::SlippageValid.should_refund(), false);
    }
}
