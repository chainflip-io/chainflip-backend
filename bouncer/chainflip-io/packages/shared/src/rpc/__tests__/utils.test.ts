import { validateSwapAmount } from '../utils';

const env = {
  swapping: {
    maximumSwapAmounts: { Ethereum: { ETH: 1000000000000000000n, FLIP: null } },
  },
  ingressEgress: {
    minimumDepositAmounts: { Ethereum: { ETH: 100000000000000000n, FLIP: 0n } },
  },
} as any;

describe(validateSwapAmount, () => {
  it('fails if the amount is too small', () => {
    const result = validateSwapAmount(
      env,
      { chain: 'Ethereum', asset: 'ETH' },
      100n,
    );

    expect(result).toEqual({
      success: false,
      reason:
        'expected amount is below minimum swap amount (100000000000000000)',
    });
  });

  it('fails if the amount is too large', () => {
    const result = validateSwapAmount(
      env,
      { chain: 'Ethereum', asset: 'ETH' },
      1000000000000000001n,
    );

    expect(result).toEqual({
      success: false,
      reason:
        'expected amount is above maximum swap amount (1000000000000000000)',
    });
  });

  it('succeeds if the amount is within range', () => {
    const result = validateSwapAmount(
      env,
      { chain: 'Ethereum', asset: 'ETH' },
      100000000000000000n,
    );

    expect(result).toEqual({ success: true });
  });

  it('succeeds when their is no upper limit', () => {
    const result = validateSwapAmount(
      env,
      { chain: 'Ethereum', asset: 'FLIP' },
      1000000000000000000000000000000000000000000000000000000000000000000000000000n,
    );

    expect(result).toEqual({ success: true });
  });
});
