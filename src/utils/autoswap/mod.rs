use std::convert::TryInto;

use crate::{
    common::{Coin, GenericCoinAmount, Liquidity, LokiAmount},
    constants::LOKI_SWAP_PROCESS_FEE,
    utils,
};

use num_bigint::BigInt;

fn calc_autoswap_from_loki(
    loki_amount: LokiAmount,
    other_amount: GenericCoinAmount,
    liquidity: Liquidity,
) -> Result<(LokiAmount, GenericCoinAmount), ()> {
    if loki_amount.to_atomic() <= LOKI_SWAP_PROCESS_FEE {
        warn!("Fee exceeds staked amount");
        let stake = calc_symmetric_from_other(other_amount, liquidity);
        return Ok((stake.loki, stake.other));
    }

    let x = calc_autoswap_generalised(
        loki_amount.to_atomic(),
        other_amount.to_atomic(),
        liquidity.loki_depth,
        liquidity.depth,
        LOKI_SWAP_PROCESS_FEE,
        0,
    )?;

    let y = utils::price::calculate_output_amount(
        Coin::LOKI,
        x,
        liquidity.loki_depth,
        LOKI_SWAP_PROCESS_FEE,
        other_amount.coin_type(),
        liquidity.depth,
        0,
    )
    .unwrap_or(0);

    let loki_effective = LokiAmount::from_atomic(loki_amount.to_atomic().saturating_sub(x));
    let other_effective = GenericCoinAmount::from_atomic(
        other_amount.coin_type(),
        other_amount.to_atomic().saturating_add(y),
    );

    if y == 0 {
        debug!("Auto-swapped amount is negligible");
        let stake = calc_symmetric_from_other(other_amount, liquidity);
        Ok((stake.loki, stake.other))
    } else {
        validate_autoswap(loki_effective, other_effective, liquidity)?;
        Ok((loki_effective, other_effective))
    }
}

fn small_other_stake(other_amount: GenericCoinAmount, liquidity: Liquidity) -> bool {
    let e: BigInt = other_amount.to_atomic().into();
    let dl: BigInt = liquidity.loki_depth.into();
    let de: BigInt = liquidity.depth.into();

    // The amount of loki that we would receive after swapping all of the other coin
    let max_loki = &e * &dl * &de / ((&e + &dl) * (&e + &dl)) - BigInt::from(LOKI_SWAP_PROCESS_FEE);

    max_loki < BigInt::from(0)
}

fn calc_autoswap_to_loki(
    loki_amount: LokiAmount,
    other_amount: GenericCoinAmount,
    liquidity: Liquidity,
) -> Result<(LokiAmount, GenericCoinAmount), ()> {
    // Input fee is 0 because we are swapping
    // some other coin for Loki

    if small_other_stake(other_amount, liquidity) {
        warn!("Fee exceeds staked amount");
        let stake = calc_symmetric_from_loki(loki_amount, other_amount.coin_type(), liquidity);
        return Ok((stake.loki, stake.other));
    }

    let x = calc_autoswap_generalised(
        other_amount.to_atomic(),
        loki_amount.to_atomic(),
        liquidity.depth,
        liquidity.loki_depth,
        0,
        LOKI_SWAP_PROCESS_FEE,
    )?;

    let y = utils::price::calculate_output_amount(
        other_amount.coin_type(),
        x,
        liquidity.depth,
        0,
        Coin::LOKI,
        liquidity.loki_depth,
        LOKI_SWAP_PROCESS_FEE,
    )
    .unwrap_or(0);

    let loki_effective = LokiAmount::from_atomic(loki_amount.to_atomic().saturating_add(y));
    let other_effective = GenericCoinAmount::from_atomic(
        other_amount.coin_type(),
        other_amount.to_atomic().saturating_sub(x),
    );

    if y == 0 {
        debug!("Auto-swapped amount is negligible");
        let stake = calc_symmetric_from_loki(loki_amount, other_amount.coin_type(), liquidity);
        Ok((stake.loki, stake.other))
    } else {
        validate_autoswap(loki_effective, other_effective, liquidity)?;
        Ok((loki_effective, other_effective))
    }
}

fn calc_autoswap_generalised(
    loki_amount: u128,
    other_amount: u128,
    loki_depth: u128,
    other_depth: u128,
    input_fee: u128,
    output_fee: u128,
) -> Result<u128, ()> {
    let l: BigInt = loki_amount.into();
    let e: BigInt = other_amount.into();
    let dl: BigInt = loki_depth.into();
    let de: BigInt = other_depth.into();
    let i_fee: BigInt = input_fee.into();
    let o_fee: BigInt = output_fee.into();

    // Solving cubic equation x^3 + b * x^2 + c * x + d = 0
    let gamma = l - (e - o_fee) * &dl / de;

    let b = BigInt::from(2) * &dl - &gamma;
    let c = BigInt::from(2) * &dl * (&dl - &gamma);
    let d = -&dl * &dl * (&gamma + &i_fee);

    let delta0 = &b * &b - BigInt::from(3) * &c;

    let delta1 = BigInt::from(2) * &b * &b * &b - BigInt::from(9) * &b * &c + BigInt::from(27) * &d;

    let in_root = &delta1 * &delta1 - BigInt::from(4) * &delta0 * &delta0 * &delta0;

    let inner_c = (&delta1 + BigInt::sqrt(&in_root)) / BigInt::from(2);

    let big_c = BigInt::cbrt(&inner_c);

    let x = -(&b + &big_c + &delta0 / &big_c) / BigInt::from(3);

    let x: u128 = match x.try_into() {
        Ok(x) => x,
        Err(err) => {
            error!("Invalid autoswap amount: {}", err);
            return Err(());
        }
    };

    if x > loki_amount {
        error!(
            "Swapped amount exceeds initial amount: {}/{}",
            x, loki_amount
        );
        return Err(());
    }

    Ok(x as u128)
}

fn validate_autoswap(
    loki_effective_amount: LokiAmount,
    other_effective_amount: GenericCoinAmount,
    liquidity: Liquidity,
) -> Result<(), ()> {
    let l: BigInt = loki_effective_amount.to_atomic().into();
    let e: BigInt = other_effective_amount.to_atomic().into();

    let de: BigInt = liquidity.depth.into();
    let dl: BigInt = liquidity.loki_depth.into();

    // Error in atomic loki (easier to calculate in whole numbers)
    let error = (dl * e) / de - l - BigInt::from(1);
    let error: i128 = error.try_into().unwrap();

    dbg!(&error);

    if error.abs() > 1_000_000 {
        return Err(());
    }

    Ok(())
}

/// Determines which way the swap should go. Note that it doesn't take fees into account:
/// for now the user always pays fees even if the autoswapped amount (y) would be smaller than fee
/// payed from that amount (o_fee).
fn calc_swap_direction(
    loki_amount: LokiAmount,
    other_amount: GenericCoinAmount,
    liquidity: Liquidity,
) -> SwapDirection {
    let l: BigInt = loki_amount.to_atomic().into();
    let e: BigInt = other_amount.to_atomic().into();

    let dl: BigInt = liquidity.loki_depth.into();
    let de: BigInt = liquidity.depth.into();

    let gamma = &l * &de - &e * &dl;

    if gamma >= BigInt::from(0) {
        SwapDirection::FromLoki
    } else {
        SwapDirection::ToLoki
    }
}

#[derive(Debug, PartialEq, Eq)]
/// In which direction to perform autoswap
enum SwapDirection {
    /// Other coin to Loki
    ToLoki,
    /// Loki to other coin
    FromLoki,
}

struct EffectiveStakeAmounts {
    loki: LokiAmount,
    other: GenericCoinAmount,
}

/// Calculate the ideal amount of loki to be staked
/// together with `other` amount (to make the stake symmetrical)
fn calc_symmetric_from_other(
    other_amount: GenericCoinAmount,
    liquidity: Liquidity,
) -> EffectiveStakeAmounts {
    let e: BigInt = other_amount.to_atomic().into();
    let de: BigInt = liquidity.depth.into();
    let dl: BigInt = liquidity.loki_depth.into();

    let loki = (e * dl) / de;

    let loki: u128 = loki.try_into().expect("unexpected overflow");

    EffectiveStakeAmounts {
        loki: LokiAmount::from_atomic(loki),
        other: other_amount,
    }
}

/// Calculate the ideal amount of loki to be staked
/// together with `other` amount (to make the stake symmetrical)
fn calc_symmetric_from_loki(
    loki_amount: LokiAmount,
    other_coin: Coin,
    liquidity: Liquidity,
) -> EffectiveStakeAmounts {
    let l: BigInt = loki_amount.to_atomic().into();
    let de: BigInt = liquidity.depth.into();
    let dl: BigInt = liquidity.loki_depth.into();

    let other = (l * de) / dl;

    let other: u128 = other.try_into().expect("unexpected overflow");

    EffectiveStakeAmounts {
        loki: loki_amount,
        other: GenericCoinAmount::from_atomic(other_coin, other),
    }
}

pub(crate) fn calc_autoswap_amount(
    loki_amount: LokiAmount,
    other_amount: GenericCoinAmount,
    liquidity: Liquidity,
) -> Result<(LokiAmount, GenericCoinAmount), ()> {
    // Need to determine which way to swap:

    match calc_swap_direction(loki_amount, other_amount, liquidity) {
        SwapDirection::FromLoki => calc_autoswap_from_loki(loki_amount, other_amount, liquidity),
        SwapDirection::ToLoki => calc_autoswap_to_loki(loki_amount, other_amount, liquidity),
    }
}

#[cfg(test)]
mod tests;
