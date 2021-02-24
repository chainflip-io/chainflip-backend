use super::*;
use crate::common::{GenericCoinAmount, OxenAmount};
use rand::prelude::*;

#[cfg(test)]
mod autoswap_tests {

    use super::*;

    fn oxen(x: u64) -> OxenAmount {
        let x = x as u128 * 1_000_000_000;
        OxenAmount::from_atomic(x)
    }

    fn eth(x: u64) -> GenericCoinAmount {
        let x = x as u128 * 1_000_000_000_000_000_000;
        GenericCoinAmount::from_atomic(Coin::ETH, x)
    }

    fn make_liquidity(oxen_amount: u64, eth_amount: u64) -> Liquidity {
        let dl = oxen(oxen_amount);
        let de = eth(eth_amount);

        Liquidity {
            depth: de.to_atomic(),
            base_depth: dl.to_atomic(),
        }
    }

    #[test]
    fn test_symmetric_oxen() {
        let res = calc_symmetric_from_other(eth(100), make_liquidity(900, 1800));
        assert_eq!(res.oxen, oxen(50));
        assert_eq!(res.other, eth(100));

        let res = calc_symmetric_from_other(eth(100), make_liquidity(900, 900));
        assert_eq!(res.oxen, oxen(100));
        assert_eq!(res.other, eth(100));

        let res = calc_symmetric_from_oxen(oxen(100), Coin::ETH, make_liquidity(900, 1800));
        assert_eq!(res.oxen, oxen(100));
        assert_eq!(res.other, eth(200));

        let res = calc_symmetric_from_oxen(oxen(100), Coin::ETH, make_liquidity(900, 900));
        assert_eq!(res.oxen, oxen(100));
        assert_eq!(res.other, eth(100));
    }

    #[test]
    fn test_swap_direction() {
        let dir = calc_swap_direction(oxen(50), eth(50), make_liquidity(49, 50));
        assert_eq!(dir, SwapDirection::FromOxen);

        let dir = calc_swap_direction(oxen(50), eth(0), make_liquidity(50, 50));
        assert_eq!(dir, SwapDirection::FromOxen);

        let dir = calc_swap_direction(oxen(50), eth(50), make_liquidity(50, 49));
        assert_eq!(dir, SwapDirection::ToOxen);

        let dir = calc_swap_direction(oxen(0), eth(50), make_liquidity(50, 50));
        assert_eq!(dir, SwapDirection::ToOxen);
    }

    #[test]
    fn autoswap_to_oxen() {
        let liquidity = make_liquidity(900, 1800);

        let res = calc_autoswap_amount(oxen(10), eth(30), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn autoswap_from_oxen() {
        let liquidity = make_liquidity(900, 1800);

        let res = calc_autoswap_amount(oxen(10), eth(10), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn big_autoswap() {
        let liquidity = make_liquidity(10, 10);

        let res = calc_autoswap_amount(oxen(90_000), eth(10_000), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn small_swap_to_oxen() {
        // The swappable oxen is smaller than the oxen fee that would be payed
        let liquidity = make_liquidity(900, 900);
        let oxen = OxenAmount::from_atomic(99_900_000_000);
        let res = calc_autoswap_amount(oxen, eth(100), liquidity);
        assert!(res.is_ok());

        let (eff_oxen, eff_eth) = res.unwrap();

        assert_eq!(eff_oxen, oxen);

        assert_eq!(
            eff_eth,
            GenericCoinAmount::from_atomic(Coin::ETH, 99_900_000_000_000_000_000)
        );
    }

    #[test]
    fn small_swap_from_oxen() {
        // The swappable oxen is smaller than the oxen fee that would be payed
        let liquidity = make_liquidity(900, 900);
        let oxen_deposit = OxenAmount::from_atomic(100_100_000_000);

        // NOTE: There is slightly more than 100 oxen:
        assert_ne!(oxen_deposit, oxen(100));

        let res = calc_autoswap_amount(oxen_deposit, eth(100), liquidity);
        assert!(res.is_ok());

        let (eff_oxen, eff_eth) = res.unwrap();

        assert_eq!(eff_oxen, oxen(100));
        assert_eq!(eff_eth, eth(100));
    }

    #[test]
    fn autoswap_not_necessary() {
        // The swappable oxen is smaller than the oxen fee that would be payed
        let liquidity = make_liquidity(900, 900);

        let deposited_oxen = oxen(100);
        let deposited_eth = eth(100);

        let res = calc_autoswap_amount(deposited_oxen, deposited_eth, liquidity);
        assert!(res.is_ok());

        let (eff_oxen, eff_eth) = res.unwrap();

        assert_eq!(eff_oxen, deposited_oxen);
        assert_eq!(eff_eth, deposited_eth);

        dbg!(&res);
    }

    #[test]
    fn autoswap_only_oxen() {
        let liquidity = make_liquidity(900, 900);

        let res = calc_autoswap_amount(oxen(100), eth(0), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn autoswap_only_eth() {
        let liquidity = make_liquidity(900, 900);

        let res = calc_autoswap_amount(oxen(0), eth(100), liquidity);
        assert!(res.is_ok());
    }

    #[test]
    fn swappable_amount_exceeds_initial_amount() {
        let liquidity = make_liquidity(90, 180);

        let l = OxenAmount::from_atomic(100_000_000);
        let e = GenericCoinAmount::from_atomic(Coin::ETH, 300_000_000);

        let res = calc_autoswap_amount(l, e, liquidity);

        assert!(res.is_ok());

        let l = OxenAmount::from_atomic(100_000_000);
        let e = eth(70_000);

        let res = calc_autoswap_amount(l, e, liquidity);

        assert!(res.is_ok());
    }

    #[test]
    fn check_nan() {
        // These inputs did not work with floating point math:
        let liquidity = make_liquidity(90, 180);

        let l = oxen(70_000_000);
        let e = eth(6_681_200);

        let res = calc_autoswap_amount(l, e, liquidity);

        assert!(res.is_ok());
    }

    #[test]
    fn test_random_autoswap_ratios() {
        let liquidity = make_liquidity(90, 180);

        let mut rng = StdRng::seed_from_u64(0);

        for _ in 0..10 {
            let l = rng.gen::<u128>();
            let e = rng.gen::<u128>();

            let l = OxenAmount::from_atomic(l);
            let e = GenericCoinAmount::from_atomic(Coin::ETH, e);

            let res = calc_autoswap_amount(l, e, liquidity);

            assert!(res.is_ok());
        }
    }
}
