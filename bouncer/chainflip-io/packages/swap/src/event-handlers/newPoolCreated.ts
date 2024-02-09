import { z } from 'zod';
import { chainflipAssetEnum, unsignedInteger } from '@/shared/parsers';
import type { EventHandlerArgs } from './index';

const eventArgs = z.union([
  z.object({
    baseAsset: chainflipAssetEnum,
    quoteAsset: chainflipAssetEnum,
    feeHundredthPips: unsignedInteger,
  }),
  // support 1.0 event shape used on sisyphos
  z
    .object({
      baseAsset: chainflipAssetEnum,
      pairAsset: chainflipAssetEnum,
      feeHundredthPips: unsignedInteger,
    })
    .transform(({ baseAsset, pairAsset, feeHundredthPips }) => ({
      baseAsset,
      quoteAsset: pairAsset,
      feeHundredthPips,
    })),
  // support 0.9 event shape used on sisyphos
  z
    .object({
      unstableAsset: chainflipAssetEnum,
      feeHundredthPips: unsignedInteger,
    })
    .transform(({ unstableAsset, feeHundredthPips }) => ({
      baseAsset: unstableAsset,
      quoteAsset: 'USDC' as const,
      feeHundredthPips,
    })),
]);

export default async function newPoolCreated({
  prisma,
  event,
}: EventHandlerArgs): Promise<void> {
  const { baseAsset, quoteAsset, feeHundredthPips } = eventArgs.parse(
    event.args,
  );

  // handle pools that were created with USDC as base asset on sisyphos: https://blocks.staging/events/384977-0
  const stableAsset = baseAsset === 'USDC' ? baseAsset : quoteAsset;
  const unstableAsset = baseAsset === 'USDC' ? quoteAsset : baseAsset;

  await prisma.pool.create({
    data: {
      baseAsset: unstableAsset,
      quoteAsset: stableAsset,
      liquidityFeeHundredthPips: Number(feeHundredthPips),
    },
  });
}
