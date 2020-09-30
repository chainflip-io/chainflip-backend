use super::*;
use crate::common::{Coin, GenericCoinAmount, LokiAmount};
use rand::prelude::*;

#[cfg(test)]
mod autoswap_tests {

    use super::*;

    fn loki(x: u64) -> LokiAmount {
        let x = x as u128 * 1_000_000_000;
        LokiAmount::from_atomic(x)
    }

    fn eth(x: u64) -> GenericCoinAmount {
        let x = x as u128 * 1_000_000_000_000_000_000;
        GenericCoinAmount::from_atomic(Coin::ETH, x)
    }

    fn make_liquidity(loki_amount: u64, eth_amount: u64) -> Liquidity {
        let dl = loki(loki_amount);
        let de = eth(eth_amount);

        Liquidity {
            depth: de.to_atomic(),
            loki_depth: dl.to_atomic(),
        }
    }

    #[test]
    fn test_symmetric_loki() {
        let res = calc_symmetric_from_other(eth(100), make_liquidity(900, 1800));
        assert_eq!(res.loki, loki(50));
        assert_eq!(res.other, eth(100));

        let res = calc_symmetric_from_other(eth(100), make_liquidity(900, 900));
        assert_eq!(res.loki, loki(100));
        assert_eq!(res.other, eth(100));

        let res = calc_symmetric_from_loki(loki(100), Coin::ETH, make_liquidity(900, 1800));
        assert_eq!(res.loki, loki(100));
        assert_eq!(res.other, eth(200));

        let res = calc_symmetric_from_loki(loki(100), Coin::ETH, make_liquidity(900, 900));
        assert_eq!(res.loki, loki(100));
        assert_eq!(res.other, eth(100));
    }

    #[test]
    fn test_swap_direction() {
        let dir = calc_swap_direction(loki(50), eth(50), make_liquidity(49, 50));
        assert_eq!(dir, SwapDirection::FromLoki);

        let dir = calc_swap_direction(loki(50), eth(0), make_liquidity(50, 50));
        assert_eq!(dir, SwapDirection::FromLoki);

        let dir = calc_swap_direction(loki(50), eth(50), make_liquidity(50, 49));
        assert_eq!(dir, SwapDirection::ToLoki);

        let dir = calc_swap_direction(loki(0), eth(50), make_liquidity(50, 50));
        assert_eq!(dir, SwapDirection::ToLoki);
    }

    #[test]
    fn autoswap_to_loki() {
        let liquidity = make_liquidity(900, 1800);

        let res = calc_autoswap_amount(loki(10), eth(30), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn autoswap_from_loki() {
        let liquidity = make_liquidity(900, 1800);

        let res = calc_autoswap_amount(loki(10), eth(10), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn big_autoswap() {
        let liquidity = make_liquidity(10, 10);

        let res = calc_autoswap_amount(loki(90_000), eth(10_000), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn small_swap_to_loki() {
        // The swappable loki is smaller than the loki fee that would be payed
        let liquidity = make_liquidity(900, 900);
        let loki = LokiAmount::from_atomic(99_900_000_000);
        let res = calc_autoswap_amount(loki, eth(100), liquidity);
        assert!(res.is_ok());

        let (eff_loki, eff_eth) = res.unwrap();

        assert_eq!(eff_loki, loki);

        assert_eq!(
            eff_eth,
            GenericCoinAmount::from_atomic(Coin::ETH, 99_900_000_000_000_000_000)
        );
    }

    #[test]
    fn small_swap_from_loki() {
        // The swappable loki is smaller than the loki fee that would be payed
        let liquidity = make_liquidity(900, 900);
        let loki_stake = LokiAmount::from_atomic(100_100_000_000);

        // NOTE: There is slightly more than 100 loki:
        assert_ne!(loki_stake, loki(100));

        let res = calc_autoswap_amount(loki_stake, eth(100), liquidity);
        assert!(res.is_ok());

        let (eff_loki, eff_eth) = res.unwrap();

        assert_eq!(eff_loki, loki(100));
        assert_eq!(eff_eth, eth(100));
    }

    #[test]
    fn autoswap_not_necessary() {
        // The swappable loki is smaller than the loki fee that would be payed
        let liquidity = make_liquidity(900, 900);

        let staked_loki = loki(100);
        let staked_eth = eth(100);

        let res = calc_autoswap_amount(staked_loki, staked_eth, liquidity);
        assert!(res.is_ok());

        let (eff_loki, eff_eth) = res.unwrap();

        assert_eq!(eff_loki, staked_loki);
        assert_eq!(eff_eth, staked_eth);

        dbg!(&res);
    }

    #[test]
    fn autoswap_only_loki() {
        let liquidity = make_liquidity(900, 900);

        let res = calc_autoswap_amount(loki(100), eth(0), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn autoswap_only_eth() {
        let liquidity = make_liquidity(900, 900);

        let res = calc_autoswap_amount(loki(0), eth(100), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn swappable_amount_exceeds_initial_amount() {
        let liquidity = make_liquidity(90, 180);

        let l = LokiAmount::from_atomic(100_000_000);
        let e = GenericCoinAmount::from_atomic(Coin::ETH, 300_000_000);

        let res = calc_autoswap_amount(l, e, liquidity);

        assert!(res.is_ok());

        let l = LokiAmount::from_atomic(100_000_000);
        let e = eth(70_000);

        let res = calc_autoswap_amount(l, e, liquidity);

        assert!(res.is_ok());
    }

    #[test]
    fn check_nan() {
        // These inputs did not work with floating point math:
        let liquidity = make_liquidity(90, 180);

        let l = loki(70_000_000);
        let e = eth(6_681_200);
        let res = calc_autoswap_amount(l, e, liquidity);

        assert!(res.is_ok());
    }

    #[test]
    fn test_random_autoswap_ratios() {
        let liquidity = make_liquidity(90, 180);

        let mut rng = StdRng::seed_from_u64(0);

        for _ in 0..100 {
            let l = rng.gen::<u128>();
            let e = rng.gen::<u128>();

            dbg!(l, e);

            let l = LokiAmount::from_atomic(l);
            let e = GenericCoinAmount::from_atomic(Coin::ETH, e);

            let res = calc_autoswap_amount(l, e, liquidity);

            assert!(res.is_ok());
        }
    }
}
