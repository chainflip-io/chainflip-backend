// Tick/price conversions matching `state-chain/amm-math` (cf-amm-math):
// - Price is a fixed point number with 128 fractional bits, measured in fine quote asset per
//   fine base asset.
// - price(tick) = 1.0001^tick, valid for ticks in [MIN_TICK, MAX_TICK].
// - A pool's `currentSqrtPrice` is a Q64.96 square root: price = sqrtPrice^2 >> 64.
//
// Computed via floats internally (~15 significant digits), which is plenty for placing orders,
// then corrected against the exact fixed-point boundaries so `priceX128ToTick` is a true floor.
//
// Note that near MIN_TICK the X128 representation itself degenerates (price(MIN_TICK) ~= 1, so
// neighbouring ticks share the same integer price). Conversions are only reliable for ticks
// where the price has enough fractional bits left, roughly [-700000, MAX_TICK] - real asset
// pairs are far inside this range (e.g. a fine Eth/Usdc price of ~3e-9 is tick ~ -196000).

export const MIN_TICK = -887272;
export const MAX_TICK = 887272;

const LOG2_TICK_BASE = Math.log2(1.0001);
const FLOAT_MANTISSA_BITS = 52;

function assertValidTick(tick: number) {
  if (!Number.isInteger(tick) || tick < MIN_TICK || tick > MAX_TICK) {
    throw new Error(`Invalid tick ${tick}, expected an integer in [${MIN_TICK}, ${MAX_TICK}]`);
  }
}

/** log2 of a positive bigint with float precision. */
function log2BigInt(value: bigint): number {
  const bits = value.toString(2).length;
  const excessBits = Math.max(0, bits - (FLOAT_MANTISSA_BITS + 1));
  return Math.log2(Number(value / 2n ** BigInt(excessBits))) + excessBits;
}

export function tickToPriceX128(tick: number): bigint {
  assertValidTick(tick);
  const log2Price = tick * LOG2_TICK_BASE;
  const exponent = Math.floor(log2Price) + 128 - FLOAT_MANTISSA_BITS;
  const mantissa = BigInt(
    Math.round(2 ** (log2Price - Math.floor(log2Price) + FLOAT_MANTISSA_BITS)),
  );
  return exponent >= 0 ? mantissa * 2n ** BigInt(exponent) : mantissa / 2n ** BigInt(-exponent);
}

/** The greatest tick whose price is <= the given price, clamped to the valid tick range. */
export function priceX128ToTick(priceX128: bigint): number {
  if (priceX128 <= 0n) {
    throw new Error(`Invalid price ${priceX128}, expected a positive X128 fixed point number`);
  }
  let tick = Math.floor((log2BigInt(priceX128) - 128) / LOG2_TICK_BASE);
  tick = Math.min(Math.max(tick, MIN_TICK), MAX_TICK);
  // Correct any float error against the exact boundaries.
  while (tick < MAX_TICK && tickToPriceX128(tick + 1) <= priceX128) {
    tick += 1;
  }
  while (tick > MIN_TICK && tickToPriceX128(tick) > priceX128) {
    tick -= 1;
  }
  return tick;
}

export function sqrtPriceX96ToPriceX128(sqrtPriceX96: bigint): bigint {
  return (sqrtPriceX96 * sqrtPriceX96) / 2n ** 64n;
}

export function sqrtPriceX96ToTick(sqrtPriceX96: bigint): number {
  return priceX128ToTick(sqrtPriceX96ToPriceX128(sqrtPriceX96));
}

/**
 * The tick for an order that beats the current pool price by one tick, so it fills before the
 * pool's range liquidity:
 * - a Buy order (buying base with quote, filling swaps that sell the base asset) wins by
 *   bidding one tick above the pool price;
 * - a Sell order wins by asking one tick below it.
 */
export function winningTick(poolSqrtPriceX96: bigint, side: 'Buy' | 'Sell'): number {
  const poolTick = sqrtPriceX96ToTick(poolSqrtPriceX96);
  const tick = side === 'Buy' ? poolTick + 1 : poolTick - 1;
  return Math.min(Math.max(tick, MIN_TICK), MAX_TICK);
}
