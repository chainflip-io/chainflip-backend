import { Asset, Assets } from '@/shared/enums';
import prisma, { Pool } from '@/swap/client';

export const getPools = async (
  srcAsset: Asset,
  destAsset: Asset,
): Promise<Pool[]> => {
  if (srcAsset === Assets.USDC || destAsset === Assets.USDC) {
    return [
      await prisma.pool.findUniqueOrThrow({
        where: {
          baseAsset_quoteAsset: {
            baseAsset: srcAsset === Assets.USDC ? destAsset : srcAsset,
            quoteAsset: srcAsset === Assets.USDC ? srcAsset : destAsset,
          },
        },
      }),
    ];
  }

  return Promise.all([
    prisma.pool.findUniqueOrThrow({
      where: {
        baseAsset_quoteAsset: {
          baseAsset: srcAsset,
          quoteAsset: Assets.USDC,
        },
      },
    }),
    prisma.pool.findUniqueOrThrow({
      where: {
        baseAsset_quoteAsset: {
          baseAsset: destAsset,
          quoteAsset: Assets.USDC,
        },
      },
    }),
  ]);
};
