use std::collections::HashMap;

use crate::{
    common::coins::PoolCoin, vault::transactions::Liquidity, vault::transactions::LiquidityProvider,
};

/// Test liquidity provider
pub struct TestLiquidityProvider {
    pools: HashMap<PoolCoin, Liquidity>,
}

impl TestLiquidityProvider {
    /// Create a new provider
    pub fn new() -> Self {
        TestLiquidityProvider {
            pools: HashMap::new(),
        }
    }

    /// Set the liquidity for a coin
    pub fn set_liquidity(&mut self, coin: PoolCoin, liquidity: Option<Liquidity>) {
        match liquidity {
            Some(amount) => self.pools.insert(coin, amount),
            None => self.pools.remove(&coin),
        };
    }
}

impl LiquidityProvider for TestLiquidityProvider {
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity> {
        self.pools.get(&pool).cloned()
    }
}
