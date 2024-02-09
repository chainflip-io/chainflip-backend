import axios from 'axios';
import { swapRate } from '@/shared/tests/fixtures';
import {
  calculateIncludedSwapFees,
  estimateIngressEgressFeeAssetAmount,
} from '@/swap/utils/fees';
import prisma from '../../client';

jest.mock('@/shared/consts', () => ({
  ...jest.requireActual('@/shared/consts'),
  getPoolsNetworkFeeHundredthPips: jest.fn().mockReturnValue(1000),
}));

jest.mock('axios', () => ({
  post: jest.fn((url, data) => {
    if (data.method === 'cf_swap_rate') {
      return Promise.resolve({
        data: swapRate({
          output: `0x${(BigInt(data.params[2]) * 2n).toString(16)}`,
        }),
      });
    }

    throw new Error(`unexpected axios call to ${url}: ${JSON.stringify(data)}`);
  }),
}));

describe('fees', () => {
  describe(calculateIncludedSwapFees, () => {
    beforeAll(async () => {
      await prisma.$queryRaw`TRUNCATE TABLE public."Pool" CASCADE`;
      await prisma.pool.createMany({
        data: [
          {
            baseAsset: 'FLIP',
            quoteAsset: 'USDC',
            liquidityFeeHundredthPips: 1000,
          },
          {
            baseAsset: 'ETH',
            quoteAsset: 'USDC',
            liquidityFeeHundredthPips: 1000,
          },
        ],
      });
    });

    it('returns fees for quote with intermediate amount', async () => {
      const fees = await calculateIncludedSwapFees(
        'ETH',
        'FLIP',
        (100e18).toString(),
        (100e6).toString(),
        (100e18).toString(),
      );

      expect(fees).toMatchObject([
        {
          type: 'NETWORK',
          asset: 'USDC',
          amount: (0.1e6).toString(),
        },
        {
          type: 'LIQUIDITY',
          asset: 'ETH',
          amount: (0.1e18).toString(),
        },
        {
          type: 'LIQUIDITY',
          asset: 'USDC',
          amount: (0.1e6).toString(),
        },
      ]);
    });

    it('returns fees for quote from USDC', async () => {
      const fees = await calculateIncludedSwapFees(
        'USDC',
        'FLIP',
        (100e6).toString(),
        undefined,
        (100e18).toString(),
      );

      expect(fees).toMatchObject([
        {
          type: 'NETWORK',
          asset: 'USDC',
          amount: (0.1e6).toString(),
        },
        {
          type: 'LIQUIDITY',
          asset: 'USDC',
          amount: (0.1e6).toString(),
        },
      ]);
    });

    it('returns fees for quote to USDC', async () => {
      const fees = await calculateIncludedSwapFees(
        'ETH',
        'USDC',
        (100e18).toString(),
        undefined,
        (99.9e6).toString(),
      );

      expect(fees).toMatchObject([
        {
          type: 'NETWORK',
          asset: 'USDC',
          amount: (0.1e6).toString(),
        },
        {
          type: 'LIQUIDITY',
          asset: 'ETH',
          amount: (0.1e18).toString(),
        },
      ]);
    });
  });

  describe(estimateIngressEgressFeeAssetAmount, () => {
    it('returns the same amount for the native asset', async () => {
      const result = await estimateIngressEgressFeeAssetAmount(
        100n,
        'ETH',
        undefined,
      );

      expect(result).toBe(100n);
      expect(axios.post).not.toBeCalled();
    });
    it('returns the rate from the rpc for a non native asset', async () => {
      const result = await estimateIngressEgressFeeAssetAmount(
        100n,
        'USDC',
        undefined,
      );

      expect(result).toBe(200n);
      expect(
        jest.mocked(axios.post).mock.calls.map((call) => call[1]),
      ).toMatchSnapshot();
    });
    it('returns the rate from the rpc for a non native asset and a block hash', async () => {
      const result = await estimateIngressEgressFeeAssetAmount(
        100n,
        'USDC',
        '0x8a741d03ae637a115ec7384c85e565799123f6a626414471260bc6d4e87d2d27',
      );

      expect(result).toBe(200n);
      expect(
        jest.mocked(axios.post).mock.calls.map((call) => call[1]),
      ).toMatchSnapshot();
    });
  });
});
