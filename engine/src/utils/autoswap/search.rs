use crate::{
    common::{GenericCoinAmount, Liquidity, OxenAmount},
    utils,
};
use chainflip_common::types::coin::Coin;
use num_bigint::{BigInt, Sign};
use std::convert::TryInto;

#[derive(Debug, Clone, Copy)]
struct OxenError(i128);

fn try_x_inner(
    x0: u128,
    input_coin: Coin,
    input_amount: u128,
    output_coin: Coin,
    output_amount: u128,
    input_depth: u128,
    output_depth: u128,
    ifee: u128,
    ofee: u128,
) -> Trial {
    assert!(x0 >= ifee);

    let y = utils::price::calculate_output_amount(
        input_coin,
        x0,
        input_depth,
        ifee,
        output_coin,
        output_depth,
        ofee,
    )
    .unwrap_or(0);

    let x: BigInt = x0.into();
    let l: BigInt = input_amount.into();
    let ifee: BigInt = ifee.into();
    let ofee: BigInt = ofee.into();
    let dl: BigInt = input_depth.into();
    let de: BigInt = output_depth.into();

    // the amount of the other coin we would contribute
    let e2: BigInt = output_amount.saturating_add(y).into();

    let y: BigInt = y.into();

    // Liquidity after the "swap":
    let dl2 = &dl + &x - &ifee;
    let de2 = &de - &y - &ofee;

    // the amount of oxen proportional to e2
    let l_target = e2 * dl2 / de2;

    // the amount of oxen left
    let l2 = l - x;

    let error = &l_target - &l2;

    let sign = error.sign();

    let error: i128 = match error.try_into() {
        Ok(x) => x,
        Err(_) => match sign {
            Sign::Plus => std::i128::MAX,
            _ => std::i128::MIN,
        },
    };

    Trial { x: x0, error }
}

fn try_x(x: u128, s: &State) -> Trial {
    try_x_inner(
        x,
        s.input_coin,
        s.input,
        s.output_coin,
        s.output,
        s.input_depth,
        s.output_depth,
        s.ifee,
        s.ofee,
    )
}

/// Point x and error associated with it
#[derive(Debug, Clone, Copy)]
struct Trial {
    x: u128,
    error: i128,
}

/// A state that gets passed around (unmodified)
/// throughout one autoswap computation
struct State {
    input: u128,
    input_coin: Coin,
    output: u128,
    output_coin: Coin,
    input_depth: u128,
    output_depth: u128,
    ifee: u128,
    ofee: u128,
}

/// Whether two numbers have the same sign
fn same_sign(x: i128, y: i128) -> bool {
    x > 0 && y > 0 || x < 0 && y < 0
}

fn search_step(t0: Trial, t2: Trial, state: &State) -> Result<(Trial, Trial), ()> {
    assert!(t2.x >= t0.x);

    // Find middle point
    let x1 = t0.x + (t2.x - t0.x) / 2;

    let t1 = try_x(x1, &state);

    if t1.error == 0 {
        return Ok((t1, t1));
    }

    // Choose the two points whose error differ in sign
    if !same_sign(t0.error, t1.error) {
        Ok((t0, t1))
    } else if !same_sign(t1.error, t2.error) {
        Ok((t1, t2))
    } else {
        debug!("The solution is not between the two points");
        Err(())
    }
}

fn find_x(x0: u128, x2: u128, state: &State) -> Result<u128, ()> {
    let mut t0 = try_x(x0, &state);
    let mut t2 = try_x(x2, &state);

    loop {
        if t2.x - t0.x <= 1 {
            break;
        }
        let res = search_step(t0, t2, &state)?;
        t0 = res.0;
        t2 = res.1;
    }

    let x = if t2.error > t0.error { t0.x } else { t2.x };

    Ok(x)
}

/// Find atomic amount `x` of Oxen that should be swapped
/// for the other coin in autoswap
pub(super) fn find_oxen_x(
    oxen: OxenAmount,
    other: GenericCoinAmount,
    liquidity: Liquidity,
    ifee: u128,
) -> Result<u128, ()> {
    let x0 = ifee;
    let x2 = oxen.to_atomic();

    let dl = liquidity.base_depth;
    let de = liquidity.depth;

    let state = State {
        input: oxen.to_atomic(),
        input_coin: Coin::OXEN,
        output_coin: other.coin_type(),
        output: other.to_atomic(),
        input_depth: dl,
        output_depth: de,
        ifee,
        ofee: 0,
    };

    find_x(x0, x2, &state)
}

/// Find atomic amount `x` of the "non-oxen" coin that should be swapped
/// for Oxen in autoswap
pub(super) fn find_other_x(
    oxen: OxenAmount,
    other_amount: GenericCoinAmount,
    liquidity: Liquidity,
    ofee: u128,
) -> Option<u128> {
    let oxen = oxen.to_atomic();
    let other = other_amount.to_atomic();
    let dl = liquidity.base_depth;
    let de = liquidity.depth;

    // min amount of other coin to swap to get non-negative oxen as output:
    let x0 = find_min_other(dl, de, ofee).ok()?;

    if x0 > other {
        return None;
    }

    let x2 = other;

    let state = State {
        input: other,
        input_coin: other_amount.coin_type(),
        output_coin: Coin::OXEN,
        output: oxen,
        input_depth: de,
        output_depth: dl,
        ifee: 0,
        ofee,
    };

    find_x(x0, x2, &state).ok()
}

/// Find minimum amount of the non-oxen coin to swap to get a non-negative oxen output
/// (i.e. taking ouptut fee into account)
fn find_min_other(dl: u128, de: u128, ofee: u128) -> Result<u128, &'static str> {
    let dl: BigInt = dl.into();
    let de: BigInt = de.into();
    let ofee: BigInt = ofee.into();

    // Solving quadratic equation x^2 + p * x + q = 0, obtained
    // from solving the price equation (`calculate_output_amount`)
    // for f(x) = OXEN_SWAP_PROCESS_FEE

    let p = BigInt::from(2) * &de - (&dl * &de) / &ofee;
    let q = &de * &de;

    let discriminant = &p * &p - BigInt::from(4) * &q;

    // discriminant is non-negative as long as de >= 4*ofee
    if discriminant < BigInt::from(0) {
        eprintln!("Negative discriminant");
        return Err("Negative discriminant");
    }

    let droot = BigInt::sqrt(&discriminant);

    // We know that only the smaller root makes sense: it is possible
    // at some point (when the input coin amount is >> de), contributing more
    // results in a smaller output, but we are not interested in doing that.
    // let x1 = (-&p + &droot) / 2;
    let x2 = (-&p - &droot) / BigInt::from(2);

    if x2 < BigInt::from(0) {
        return Err("underflow");
    }

    x2.try_into().map_err(|_| "overflow")
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{common::GenericCoinAmount, constants::OXEN_SWAP_PROCESS_FEE, utils};

    fn oxen(x: u128) -> OxenAmount {
        OxenAmount::from_atomic(x * 1_000_000_000)
    }

    fn eth(x: u128) -> GenericCoinAmount {
        GenericCoinAmount::from_atomic(Coin::ETH, x * 1_000_000_000_000_000_000)
    }

    fn liquidity(l: OxenAmount, e: GenericCoinAmount) -> Liquidity {
        Liquidity::new(e.to_atomic(), l.to_atomic())
    }

    #[test]
    fn negative_discriminant() {
        let de = eth(100).to_atomic();
        let dl = OXEN_SWAP_PROCESS_FEE * 4;
        let ofee = OXEN_SWAP_PROCESS_FEE;

        find_min_other(dl, de, ofee).unwrap();

        // de < 4 * output fee
        let res = find_min_other(dl - 1, de, ofee).unwrap_err();
        assert_eq!(res, "Negative discriminant");
    }

    #[test]
    fn min_swappable_amount() {
        let dl = oxen(200).to_atomic();
        let de = eth(200).to_atomic();

        let x = find_min_other(dl, de, OXEN_SWAP_PROCESS_FEE).unwrap();

        let y = utils::price::calculate_output_amount(
            Coin::ETH,
            x,
            de,
            0,
            Coin::OXEN,
            dl,
            OXEN_SWAP_PROCESS_FEE,
        )
        .unwrap_or(0);

        assert_eq!(y, 0);
    }

    #[test]
    fn test_same_sign() {
        assert!(same_sign(-10, -20));
        assert!(same_sign(10, 20));
        assert!(!same_sign(10, -20));
        assert!(!same_sign(-10, 20));
    }

    #[test]
    fn search_oxen_x() {
        let l = oxen(1_000_000);
        let e = eth(2_000);
        let dl = oxen(20_000_000);
        let de = eth(100_000);

        assert!(find_oxen_x(l, e, liquidity(dl, de), OXEN_SWAP_PROCESS_FEE).is_ok());
    }

    #[test]
    fn search_other_x() {
        let l = oxen(1_000_000);
        let e = eth(20_000);
        let dl = oxen(20_000_000);
        let de = eth(100_000);

        assert!(find_other_x(l, e, liquidity(dl, de), OXEN_SWAP_PROCESS_FEE).is_some());
    }
}
