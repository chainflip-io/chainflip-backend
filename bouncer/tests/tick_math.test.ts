import { describe, expect, it } from 'vitest';
import {
  MAX_TICK,
  priceX128ToTick,
  sqrtPriceX96ToPriceX128,
  sqrtPriceX96ToTick,
  tickToPriceX128,
  winningTick,
} from 'shared/live/tick_math';

// Pure unit tests, no localnet required.
describe('tick_math', () => {
  it('converts tick 0 to price 1 exactly', () => {
    expect(tickToPriceX128(0)).toBe(2n ** 128n);
  });

  it('floors prices to the enclosing tick', () => {
    expect(priceX128ToTick(2n ** 128n)).toBe(0);
    expect(priceX128ToTick(2n ** 128n - 1n)).toBe(-1);
    expect(priceX128ToTick(2n ** 128n + 1n)).toBe(0);
    // ln(2) / ln(1.0001) = 6931.8..., so price 2 lies within tick 6931.
    expect(priceX128ToTick(2n ** 129n)).toBe(6931);
  });

  // Round-trip guarantees only hold where the X128 price keeps enough fractional bits (see the
  // note in tick_math.ts); -196000 is the realistic Eth/Usdc region.
  it('round-trips ticks through prices', () => {
    for (const tick of [-700000, -196000, -100000, -6932, -1, 0, 1, 6931, 100000, MAX_TICK]) {
      expect(priceX128ToTick(tickToPriceX128(tick))).toBe(tick);
    }
  });

  it('produces strictly increasing prices', () => {
    for (const tick of [-700000, -196000, -50000, -1, 0, 1, 50000, MAX_TICK - 1]) {
      expect(tickToPriceX128(tick + 1) > tickToPriceX128(tick)).toBe(true);
    }
  });

  it('converts sqrt prices', () => {
    // sqrtPrice 2^96 (= 1.0) -> price 1.0 -> tick 0
    expect(sqrtPriceX96ToPriceX128(2n ** 96n)).toBe(2n ** 128n);
    expect(sqrtPriceX96ToTick(2n ** 96n)).toBe(0);
    // sqrtPrice for price 4 is 2 * 2^96
    expect(sqrtPriceX96ToPriceX128(2n ** 97n)).toBe(2n ** 130n);
  });

  it('rejects out-of-range inputs', () => {
    expect(() => tickToPriceX128(MAX_TICK + 1)).toThrow();
    expect(() => tickToPriceX128(0.5)).toThrow();
    expect(() => priceX128ToTick(0n)).toThrow();
  });

  it('computes winning ticks one tick better than the pool price', () => {
    expect(winningTick(2n ** 96n, 'Buy')).toBe(1);
    expect(winningTick(2n ** 96n, 'Sell')).toBe(-1);
  });
});
