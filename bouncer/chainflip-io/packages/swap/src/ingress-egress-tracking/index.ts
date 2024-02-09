import { Asset, Chain, Chains } from '@/shared/enums';
import RedisClient from '@/shared/node-apis/redis';
import prisma, { Broadcast } from '../client';
import env from '../config/env';
import { handleExit } from '../utils/function';
import logger from '../utils/logger';

const redis = env.REDIS_URL ? new RedisClient(env.REDIS_URL) : undefined;

if (redis) handleExit(() => redis.quit());

export type PendingDeposit = {
  amount: string;
  transactionHash?: string;
  transactionConfirmations: number;
};

const getMempoolTransaction = async (
  chain: Chain,
  address: string,
): Promise<PendingDeposit | null> => {
  if (!redis) return null;

  if (chain === Chains.Bitcoin) {
    const tx = await redis.getMempoolTransaction('Bitcoin', address);

    if (tx) {
      return {
        amount: tx.value.toString(),
        transactionConfirmations: tx.confirmations,
        transactionHash: tx.tx_hash,
      };
    }
  }

  return null;
};

export const getPendingDeposit = async (
  chain: Chain,
  asset: Asset,
  address: string,
): Promise<PendingDeposit | null> => {
  if (!redis) return null;

  try {
    const [deposits, tracking] = await Promise.all([
      redis.getDeposits(chain, asset, address),
      prisma.chainTracking.findFirst({ where: { chain } }),
    ]);

    if (deposits.length === 0 || tracking == null) {
      return getMempoolTransaction(chain, address);
    }

    const confirmations = Math.max(
      0,
      Number(tracking.height) - deposits[0].deposit_chain_block_height + 1,
    );

    return {
      amount: deposits[0].amount.toString(),
      transactionConfirmations: confirmations,
    };
  } catch (error) {
    logger.error('error while looking up deposit in redis', { error });
    return null;
  }
};

export const getPendingBroadcast = async (broadcast: Broadcast) => {
  if (!redis) return null;
  try {
    return await redis.getBroadcast(broadcast.chain, broadcast.nativeId);
  } catch (error) {
    logger.error('error while looking up broadcast in redis', { error });
    return null;
  }
};
