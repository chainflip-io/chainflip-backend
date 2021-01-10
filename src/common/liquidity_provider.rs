use chainflip_common::types::{chain::PoolChange, coin::Coin};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryInto};

use crate::common::coins::PoolCoin;

/// A simple representation of a pool liquidity
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquidity {
    /// The depth of the coin staked against Base coin in the pool
    pub depth: u128,
    /// The depth of Base coin in the pool
    pub base_depth: u128,
}

impl Liquidity {
    /// Create a new liquidity
    pub fn new(depth: u128, base_depth: u128) -> Self {
        Liquidity { depth, base_depth }
    }

    /// Create a liquidity with zero amount
    pub fn zero() -> Self {
        Self::new(0, 0)
    }
}

/// An interface for providing liquidity
pub trait LiquidityProvider {
    /// Get the liquidity for a given pool
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity>;
}

/// An in-memory liquidity provider
#[derive(Debug)]
pub struct MemoryLiquidityProvider {
    pools: HashMap<PoolCoin, Liquidity>,
}

impl MemoryLiquidityProvider {
    /// Create a new memory liquidity provider
    pub fn new() -> Self {
        MemoryLiquidityProvider {
            pools: HashMap::new(),
        }
    }

    /// Get the current pools
    pub fn get_pools(&self) -> &HashMap<PoolCoin, Liquidity> {
        &self.pools
    }

    /// Populate liquidity from another provider.
    /// **This will overwrite existing values.**
    pub fn populate<L: LiquidityProvider>(&mut self, other: &L) {
        let coins: Vec<PoolCoin> = Coin::SUPPORTED
            .iter()
            .filter_map(|c| PoolCoin::from(c.clone()).ok())
            .collect();

        for coin in coins {
            self.set_liquidity(coin, other.get_liquidity(coin));
        }
    }

    /// Set the liquidity
    pub fn set_liquidity(&mut self, coin: PoolCoin, liquidity: Option<Liquidity>) {
        match liquidity {
            Some(amount) => self.pools.insert(coin, amount),
            None => self.pools.remove(&coin),
        };
    }

    /// Update liquidity from a pool change transaction
    pub fn update_liquidity(&mut self, pool_change: &PoolChange) -> Result<(), &'static str> {
        let pool_coin = pool_change.pool.try_into()?;

        let mut liquidity = self
            .pools
            .get(&pool_coin)
            .cloned()
            .unwrap_or(Liquidity::zero());

        let depth = liquidity.depth as i128 + pool_change.depth_change;
        let base_depth = liquidity.base_depth as i128 + pool_change.base_depth_change;
        if depth < 0 || base_depth < 0 {
            return Err("Negative liquidity depth found");
        }

        liquidity.depth = depth as u128;
        liquidity.base_depth = base_depth as u128;

        debug!(
            "Liquidity for coin {:?} is now: {:?}",
            pool_change.pool, liquidity
        );

        self.pools.insert(pool_coin, liquidity);

        Ok(())
    }
}

impl LiquidityProvider for MemoryLiquidityProvider {
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity> {
        self.pools.get(&pool).cloned()
    }
}

#[cfg(test)]
mod test {
    use crate::utils::test_utils::data::TestData;

    use super::*;

    #[test]
    fn returns_liquidity() {
        let mut provider = MemoryLiquidityProvider::new();

        assert!(provider.get_liquidity(PoolCoin::ETH).is_none());

        provider.set_liquidity(PoolCoin::ETH, Some(Liquidity::zero()));

        assert_eq!(
            provider.get_liquidity(PoolCoin::ETH),
            Some(Liquidity::zero())
        );
    }

    #[test]
    fn populates_liquidity_correctly() {
        let mut provider = MemoryLiquidityProvider::new();
        let mut other = MemoryLiquidityProvider::new();

        let liquidity = Liquidity::new(100, 200);
        other.set_liquidity(PoolCoin::ETH, Some(liquidity));

        assert!(provider.get_liquidity(PoolCoin::ETH).is_none());

        provider.populate(&other);

        assert_eq!(provider.get_liquidity(PoolCoin::ETH), Some(liquidity));
        assert!(provider.get_liquidity(PoolCoin::BTC).is_none());
    }

    #[test]
    fn updates_liquidity() {
        let mut provider = MemoryLiquidityProvider::new();
        let pool_change = TestData::pool_change(Coin::ETH, -100, 100);

        assert!(provider.get_liquidity(PoolCoin::ETH).is_none());
        assert_eq!(
            provider.update_liquidity(&pool_change).unwrap_err(),
            "Negative liquidity depth found"
        );

        provider.set_liquidity(PoolCoin::ETH, Some(Liquidity::new(100, 100)));
        assert!(provider.update_liquidity(&pool_change).is_ok());
        assert_eq!(
            provider.get_liquidity(PoolCoin::ETH),
            Some(Liquidity::new(0, 200))
        )
    }
}
