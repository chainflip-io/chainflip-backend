import { Assets } from '@/shared/enums';
import { getPools } from '@/swap/utils/pools';
import prisma from '../../client';

jest.mock('@/shared/consts', () => ({
  ...jest.requireActual('@/shared/consts'),
  getPoolsNetworkFeeHundredthPips: jest.fn().mockReturnValue(1000),
}));

describe('pools', () => {
  describe(getPools, () => {
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
            liquidityFeeHundredthPips: 2000,
          },
        ],
      });
    });

    it('returns pools for quote with intermediate amount', async () => {
      const pools = await getPools('FLIP', 'ETH');

      expect(pools).toHaveLength(2);
      expect(pools[0]).toMatchObject({
        baseAsset: Assets.FLIP,
        quoteAsset: Assets.USDC,
      });
      expect(pools[1]).toMatchObject({
        baseAsset: Assets.ETH,
        quoteAsset: Assets.USDC,
      });
    });

    it('returns pools for quote from USDC', async () => {
      const pools = await getPools('USDC', 'ETH');

      expect(pools).toHaveLength(1);
      expect(pools[0]).toMatchObject({
        baseAsset: Assets.ETH,
        quoteAsset: Assets.USDC,
      });
    });

    it('returns pools for quote to USDC', async () => {
      const pools = await getPools('FLIP', 'USDC');

      expect(pools).toHaveLength(1);
      expect(pools[0]).toMatchObject({
        baseAsset: Assets.FLIP,
        quoteAsset: Assets.USDC,
      });
    });
  });
});
