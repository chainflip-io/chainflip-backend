import { Assets, Chains } from '../../enums';
import {
  NativeSwapParams,
  TokenSwapParams,
  executeSwapParamsSchema,
} from '../schemas';

const ETH_ADDRESS = '0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2';
const DOT_ADDRESS = '5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX';
const BTC_ADDRESS = 'tb1qge9vvd2mmjxfhuxuq204h4fxxphr0vfnsnx205';

const parse = (params: unknown): boolean =>
  executeSwapParamsSchema('perseverance').safeParse(params).success;

describe('executeSwapParamsSchema', () => {
  it.each([
    {
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destChain: Chains.Bitcoin,
      destAddress: BTC_ADDRESS,
      destAsset: Assets.BTC,
    },
    {
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destChain: Chains.Polkadot,
      destAddress: DOT_ADDRESS,
      destAsset: Assets.DOT,
    },
    ...[Assets.FLIP, Assets.USDC].map((destAsset) => ({
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
      destAsset,
    })),
  ] as Omit<NativeSwapParams, 'amount'>[])(
    'accepts valid native swaps (%p)',
    (params) => {
      expect(parse({ amount: '1', ...params })).toBe(true);
    },
  );

  it.each([
    {
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destChain: Chains.Bitcoin,
      destAddress: '0xOoOoOo',
      destAsset: Assets.BTC,
    },
    {
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destChain: Chains.Polkadot,
      destAddress: '0xOoOoOo',
      destAsset: Assets.DOT,
    },
    ...[Assets.FLIP, Assets.USDC].map((destAsset) => ({
      srcAsset: Assets.ETH,
      srcChain: Chains.Ethereum,
      destChain: Chains.Ethereum,
      destAddress: '0xOoOoOo',
      destAsset,
    })),
  ] as Omit<NativeSwapParams, 'amount'>[])(
    'rejects native swaps without an amount (%p)',
    (params) => {
      expect(parse(params)).toBe(false);
    },
  );

  it.each([
    ...[Assets.FLIP, Assets.USDC, Assets.ETH, Assets.DOT].map((destAsset) => ({
      destChain: Chains.Bitcoin,
      destAddress: BTC_ADDRESS,
      destAsset,
    })),
    ...[Assets.FLIP, Assets.USDC, Assets.ETH, Assets.BTC].map((destAsset) => ({
      destChain: Chains.Polkadot,
      destAddress: BTC_ADDRESS,
      destAsset,
    })),
    ...[Assets.DOT, Assets.BTC, Assets.ETH].map((destAsset) => ({
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
      destAsset,
    })),
  ] as Omit<NativeSwapParams, 'amount'>[])(
    'rejects native swaps with mismatching chains and assets (%p)',
    (params) => {
      expect(parse({ amount: '1', ...params })).toBe(false);
    },
  );

  it.each([
    ...(
      [
        {
          srcAsset: Assets.USDC,
          srcChain: Chains.Ethereum,
        },
        {
          srcAsset: Assets.FLIP,
          srcChain: Chains.Ethereum,
        },
      ] as const
    ).flatMap((src) => [
      {
        destChain: Chains.Bitcoin,
        destAddress: BTC_ADDRESS,
        destAsset: Assets.BTC,
        ...src,
      },
      {
        destChain: Chains.Polkadot,
        destAddress: DOT_ADDRESS,
        destAsset: Assets.DOT,
        ...src,
      },
      {
        destChain: Chains.Ethereum,
        destAddress: ETH_ADDRESS,
        destAsset: Assets.ETH,
        ...src,
      },
    ]),
    {
      srcChain: Chains.Ethereum,
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
      destAsset: Assets.USDC,
      srcAsset: Assets.FLIP,
    },
    {
      srcChain: Chains.Ethereum,
      destChain: Chains.Ethereum,
      destAddress: ETH_ADDRESS,
      destAsset: Assets.FLIP,
      srcAsset: Assets.USDC,
    },
  ] as Omit<TokenSwapParams, 'amount'>[])(
    'accepts valid token swaps (%p)',
    (params) => {
      expect(parse({ amount: '1', ...params })).toBe(true);
    },
  );

  it.each([
    ...[Assets.ETH, Assets.DOT, Assets.BTC].flatMap((srcAsset) => [
      {
        destChain: Chains.Bitcoin,
        destAddress: BTC_ADDRESS,
        destAsset: Assets.BTC,
        srcAsset,
      },
      {
        destChain: Chains.Polkadot,
        destAddress: DOT_ADDRESS,
        destAsset: Assets.DOT,
        srcAsset,
      },
      {
        destChain: Chains.Ethereum,
        destAddress: ETH_ADDRESS,
        destAsset: Assets.ETH,
        srcAsset,
      },
    ]),
  ] as Omit<NativeSwapParams, 'amount'>[])(
    'rejects tokens swaps with invalid srcAssets (%p)',
    (params) => {
      expect(parse({ amount: '1', ...params })).toBe(false);
    },
  );
});
