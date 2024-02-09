import { z } from 'zod';
import { Asset, assetChains } from '@/shared/enums';
import { bigintMin } from '@/shared/functions';
import { chainflipChain, u128, unsignedInteger } from '@/shared/parsers';
import { Environment, getEnvironment } from '@/shared/rpc';
import { readAssetValue } from '@/shared/rpc/utils';
import { CacheMap } from '@/swap/utils/dataStructures';
import { estimateIngressEgressFeeAssetAmount } from '@/swap/utils/fees';
import env from '../config/env';
import type { EventHandlerArgs } from '.';

const eventArgsWithoutFee = z.object({
  swapId: unsignedInteger,
  egressId: z.tuple([
    z.object({ __kind: chainflipChain }).transform(({ __kind }) => __kind),
    unsignedInteger,
  ]),
});

const v120EventArgs = eventArgsWithoutFee.and(
  z.object({ amount: u128, fee: u128 }),
);

const eventArgs = z.union([v120EventArgs, eventArgsWithoutFee]);

const environmentByBlockHashCache = new CacheMap<
  string,
  Promise<Environment | null>
>(60_000);

const methodNotFoundRegExp = /Exported method .+ is not found/;
const rpcConfig = { rpcUrl: env.RPC_NODE_HTTP_URL };

const getCachedEnvironmentAtBlock = async (
  blockHash: string,
): Promise<Environment | null> => {
  const cached = environmentByBlockHashCache.get(blockHash);
  if (cached) return cached;

  const environment = getEnvironment(rpcConfig, blockHash).catch((e: Error) => {
    const cause = e.cause as { message: string } | undefined;

    if (methodNotFoundRegExp.test(cause?.message ?? '')) return null;

    environmentByBlockHashCache.delete(blockHash);
    throw e;
  });

  environmentByBlockHashCache.set(blockHash, environment);

  return environment;
};

const assetAmountByNativeAmountAndBlockHashCache = new CacheMap<
  string,
  Promise<bigint>
>(60_000);

const getCachedAssetAmountAtBlock = async (
  asset: Asset,
  nativeAmount: bigint,
  blockHash: string,
): Promise<bigint> => {
  const cacheKey = `${asset}-${nativeAmount.toString()}-${blockHash}`;
  const cached = assetAmountByNativeAmountAndBlockHashCache.get(cacheKey);
  if (cached) return cached;

  const rate = estimateIngressEgressFeeAssetAmount(
    nativeAmount,
    asset,
    blockHash,
  ).catch((e: Error) => {
    assetAmountByNativeAmountAndBlockHashCache.delete(blockHash);
    throw e;
  });

  assetAmountByNativeAmountAndBlockHashCache.set(cacheKey, rate);

  return rate;
};

const getEgressFeeAtBlock = async (
  blockHash: string,
  asset: Asset,
): Promise<bigint> => {
  const environment = await getCachedEnvironmentAtBlock(blockHash);
  if (!environment) return 0n;

  const nativeFee = readAssetValue(environment.ingressEgress.egressFees, {
    asset,
    chain: assetChains[asset],
  });

  return getCachedAssetAmountAtBlock(asset, nativeFee, blockHash);
};

/**
 * this event is emitted in order to correlate the egress id from a network
 * deposit/egress pallet to a swap id
 */
export default async function swapEgressScheduled({
  prisma,
  event,
  block,
}: EventHandlerArgs): Promise<void> {
  const {
    swapId,
    egressId: [chain, nativeId],
    ...restArgs
  } = eventArgs.parse(event.args);

  const swap = await prisma.swap.findUniqueOrThrow({
    where: { nativeId: swapId },
  });

  let egressFee;
  let egress;

  if ('fee' in restArgs) {
    egressFee = restArgs.fee;
    egress = await prisma.egress.create({
      data: {
        nativeId,
        chain,
        amount: restArgs.amount.toString(),
        scheduledAt: new Date(block.timestamp),
        scheduledBlockIndex: `${block.height}-${event.indexInBlock}`,
      },
    });
  } else {
    egressFee = bigintMin(
      await getEgressFeeAtBlock(block.hash, swap.destAsset),
      BigInt(swap.swapOutputAmount?.toFixed() ?? 0),
    );
    egress = await prisma.egress.update({
      where: { nativeId_chain: { chain, nativeId } },
      data: {
        amount: swap.swapOutputAmount?.sub(egressFee.toString()),
      },
    });
  }

  await prisma.swap.update({
    where: { nativeId: swapId },
    data: {
      egress: { connect: { id: egress.id } },
      fees: {
        create: {
          type: 'EGRESS',
          asset: swap.destAsset,
          amount: egressFee.toString(),
        },
      },
    },
  });
}
