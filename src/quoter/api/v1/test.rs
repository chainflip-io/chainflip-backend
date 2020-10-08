use super::*;

struct FakeState {}
impl StateProvider for FakeState {
    fn get_swap_quotes(&self) -> Option<Vec<crate::transactions::QuoteTx>> {
        todo!()
    }

    fn get_swap_quote_tx(&self, id: String) -> Option<crate::transactions::QuoteTx> {
        todo!()
    }

    fn get_stake_quotes(&self) -> Option<Vec<crate::transactions::StakeQuoteTx>> {
        todo!()
    }

    fn get_stake_quote_tx(&self, id: String) -> Option<crate::transactions::StakeQuoteTx> {
        todo!()
    }

    fn get_witness_txs(&self, quote_id: String) -> Option<Vec<crate::transactions::WitnessTx>> {
        todo!()
    }

    fn get_output_txs(&self, quote_id: String) -> Option<Vec<crate::transactions::OutputTx>> {
        todo!()
    }

    fn get_output_sent_txs(&self) -> Option<Vec<crate::transactions::OutputSentTx>> {
        todo!()
    }

    fn get_pools(&self) -> HashMap<crate::common::PoolCoin, crate::common::Liquidity> {
        todo!()
    }
}

#[tokio::test]
pub async fn get_coins_returns_all_coins() {
    let params = CoinsParams { symbols: None };
    let result = get_coins(params).await.expect("Expected result to be Ok.");
    assert_eq!(result.len(), Coin::SUPPORTED.len());
}

#[tokio::test]
pub async fn get_coins_returns_coin_information() {
    let params = CoinsParams {
        symbols: Some(vec![
            "eth".to_owned(),
            "LOKI".to_owned(),
            "invalid_coin".to_owned(),
        ]),
    };
    let result = get_coins(params).await.expect("Expected result to be Ok.");

    assert_eq!(result.len(), 2, "Expected get_coins to return 2 CoinInfo");

    for info in result {
        match info.symbol {
            Coin::ETH | Coin::LOKI => continue,
            coin @ _ => panic!("Result returned unexpected coin: {}", coin),
        }
    }
}

#[tokio::test]
pub async fn get_estimate_validates_params() {
    let state = Arc::new(Mutex::new(FakeState {}));

    // =============

    let invalid_input_coin = EstimateParams {
        input_coin: "invalid".to_owned(),
        input_amount: 1000000,
        output_coin: "loki".to_owned(),
    };

    let error = get_estimate(invalid_input_coin, state.clone())
        .await
        .expect_err("Expected an error");

    assert_eq!(error.message, "Invalid input coin");

    // =============

    let invalid_output_coin = EstimateParams {
        input_coin: "loki".to_owned(),
        input_amount: 1000000,
        output_coin: "invalid".to_owned(),
    };

    let error = get_estimate(invalid_output_coin, state.clone())
        .await
        .expect_err("Expected an error");

    assert_eq!(error.message, "Invalid output coin");

    // =============

    let same_coins = EstimateParams {
        input_coin: "loki".to_owned(),
        input_amount: 1000000,
        output_coin: "loki".to_owned(),
    };

    let error = get_estimate(same_coins, state.clone())
        .await
        .expect_err("Expected an error");

    assert_eq!(
        error.message,
        "Input coin must be different from output coin"
    );

    // =============

    let invalid_input_amount = EstimateParams {
        input_coin: "btc".to_owned(),
        input_amount: 0,
        output_coin: "loki".to_owned(),
    };

    let error = get_estimate(invalid_input_amount, state.clone())
        .await
        .expect_err("Expected an error");

    assert_eq!(error.message, "Input amount must be greater than 0");

    // =============

    let valid_params = EstimateParams {
        input_coin: "btc".to_owned(),
        input_amount: 100,
        output_coin: "loki".to_owned(),
    };

    get_estimate(valid_params, state.clone())
        .await
        .expect("Expected a valid result");
}
