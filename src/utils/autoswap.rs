use crate::utils;

/// Calculate the amount `x` of coin L to auto-swap for amount `y` of coin E such that
/// such that the amount of X and Y provisioned into the pool matches the current ratio
/// in the pool, i.e. `(l - x) / (e - y) = dl / de`. Given: `l` (`e`) -- the amount of
/// coin L (E) provided by the provisioner, dl (de) -- current depth of L (E) in the pool.
/// Returns `(x, y)`.
///
/// Current limitation: coin L must be provided in excess, i.e. we will be auto-swapping it
/// for L, otherwise returns an error.
pub fn calc_autoswap_amount(
    l: u128,
    e: u128,
    dl: u128,
    de: u128,
    input_fee: u128,
) -> Result<(u128, u128), ()> {
    let l = l as f64;
    let e = e as f64;
    let dl = dl as f64;
    let de = de as f64;
    let i_fee = input_fee as f64;

    let gamma = l - (e * dl / de);

    if gamma <= 0.0 {
        // We only allow gamma > 0, i.e, there is excess of l, not e.
        return Err(());
    }

    // Solving cubic equation x^3 + b * x^2 + c * x + d = 0

    let b = 2.0 * dl - gamma;
    let c = 2.0 * dl * (dl - gamma);
    let d = -dl * dl * (gamma + i_fee);

    let delta0 = b * b - 3.0 * c;
    let delta1 = 2.0 * b * b * b - 9.0 * b * c + 27.0 * d;

    let inner_c = (delta1 + f64::sqrt(delta1 * delta1 - 4.0 * delta0 * delta0 * delta0)) / 2.0;
    let big_c = f64::cbrt(inner_c);

    let x = -(b + big_c + delta0 / big_c) / 3.0;

    // Sanity checks

    if x > l || x < 0.0 {
        return Err(());
    }

    // For now we assume that the first coin (L) is Loki, so
    // there is no output fee
    let output_fee = 0.0;

    let y = utils::price::calculate_output_amount(x, dl, i_fee, de, output_fee);

    // should be as close to 0 as possible
    let error = 1.0 - ((l - x) / (e + y)) * de / dl;

    if error > 0.0001 {
        return Err(());
    }

    Ok((x as u128, y as u128))
}

#[cfg(test)]
mod tests {

    fn loki(x: u64) -> u128 {
        x as u128 * 1_000_000_000
    }

    fn eth(x: u64) -> u128 {
        x as u128 * 1_000_000_000_000_000_000
    }

    #[test]
    fn test_autoswap_ratios() {
        use super::*;
        use rand::prelude::*;

        let fee = loki(1);

        let l = loki(10);
        let e = eth(10);
        let dl = loki(90);
        let de = eth(180);

        let res = calc_autoswap_amount(l, e, dl, de, fee);
        assert!(res.is_ok());

        dbg!(&res);

        let res = calc_autoswap_amount(loki(100), 0, dl, de, fee);
        assert!(res.is_ok());

        // At the moment, it is expected that there is excess of coin on the left,
        // which isn't the case for the following parameters:
        let res = calc_autoswap_amount(loki(10), eth(30), dl, de, fee);
        assert!(res.is_err());

        let res = calc_autoswap_amount(loki(10), eth(10), loki(10), de, fee);
        assert!(res.is_ok());

        let mut rng = StdRng::seed_from_u64(0);

        for _ in 0..100 {
            let l = rng.gen::<u128>();
            let e = rng.gen::<u128>();

            let dl = rng.gen::<u128>();
            let de = rng.gen::<u128>();

            // TODO: add randomised fee
            let fee = 0;

            let gamma = l as f64 - (e as f64 * dl as f64 / de as f64);

            let res = calc_autoswap_amount(l, e, dl, de, fee);

            if gamma <= 0.0 {
                assert!(res.is_err());
            } else {
                assert!(res.is_ok());
            }
        }
    }
}
