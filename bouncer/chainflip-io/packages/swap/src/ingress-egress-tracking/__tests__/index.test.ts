import Redis from 'ioredis';
import { Chain } from '@/shared/enums';
import { getPendingBroadcast, getPendingDeposit } from '..';
import prisma, { Broadcast } from '../../client';
import logger from '../../utils/logger';

jest.mock('../../config/env', () => ({
  REDIS_URL: 'redis://localhost:6379',
}));

jest.mock('../../utils/logger');

const updateChainTracking = (data: { chain: Chain; height: bigint }) =>
  prisma.chainTracking.upsert({
    where: { chain: data.chain },
    update: data,
    create: data,
  });

describe('ingress-egress-tracking', () => {
  let redis: Redis;

  beforeAll(() => {
    redis = new Redis();
  });

  beforeEach(async () => {
    await prisma.chainTracking.deleteMany();
  });

  afterEach(async () => {
    await redis.flushall();
    await prisma.chainTracking.deleteMany();
  });

  afterAll(async () => {
    await redis.quit();
  });

  describe(getPendingDeposit, () => {
    it('gets pending non-bitcoin deposits from redis', async () => {
      await updateChainTracking({ chain: 'Ethereum', height: 1234567893n });

      await redis.rpush(
        'deposit:Ethereum:0x1234',
        JSON.stringify({
          amount: '0x9000',
          asset: 'FLIP',
          deposit_chain_block_height: 1234567890,
        }),
      );

      const deposit = await getPendingDeposit('Ethereum', 'FLIP', '0x1234');

      expect(deposit).toEqual({ amount: '36864', transactionConfirmations: 4 });
    });

    it('returns null if the non-bitcoin deposit is not found', async () => {
      await updateChainTracking({ chain: 'Ethereum', height: 1234567893n });

      const deposit = await getPendingDeposit('Ethereum', 'FLIP', '0x1234');

      expect(deposit).toBeNull();
      expect(logger.error).not.toHaveBeenCalled();
    });

    it('gets mempool txs for bitcoin from redis', async () => {
      await redis.set(
        'mempool:Bitcoin:tb1q8uzv43phxxsndlxglj74ryc6umxuzuz22u7erf',
        JSON.stringify({
          tx_hash: 'deadc0de',
          value: '0x9000',
          confirmations: 3,
        }),
      );

      const deposit = await getPendingDeposit(
        'Bitcoin',
        'BTC',
        'tb1q8uzv43phxxsndlxglj74ryc6umxuzuz22u7erf',
      );

      expect(logger.error).not.toHaveBeenCalled();
      expect(deposit).toEqual({
        amount: '36864',
        transactionConfirmations: 3,
        transactionHash: '0xdeadc0de',
      });
    });

    it('gets pending bitcoin deposits from redis', async () => {
      await Promise.all([
        redis.set(
          'mempool:Bitcoin:tb1q8uzv43phxxsndlxglj74ryc6umxuzuz22u7erf',
          JSON.stringify({
            tx_hash: 'deadc0de',
            value: '0x9000',
            confirmations: 1,
          }),
        ),
        redis.rpush(
          'deposit:Bitcoin:tb1q8uzv43phxxsndlxglj74ryc6umxuzuz22u7erf',
          JSON.stringify({
            amount: '0x9000',
            asset: 'BTC',
            deposit_chain_block_height: 1234567890,
          }),
        ),
        updateChainTracking({ chain: 'Bitcoin', height: 1234567893n }),
      ]);

      const deposit = await getPendingDeposit(
        'Bitcoin',
        'BTC',
        'tb1q8uzv43phxxsndlxglj74ryc6umxuzuz22u7erf',
      );

      expect(logger.error).not.toHaveBeenCalled();
      expect(deposit).toEqual({ amount: '36864', transactionConfirmations: 4 });
    });

    it('returns null if the non-bitcoin deposit is not found', async () => {
      const deposit = await getPendingDeposit(
        'Bitcoin',
        'BTC',
        'tb1q8uzv43phxxsndlxglj74ryc6umxuzuz22u7erf',
      );

      expect(logger.error).not.toHaveBeenCalled();
      expect(deposit).toBeNull();
    });

    it('returns null if the redis client throws (non-bitcoin)', async () => {
      jest.spyOn(Redis.prototype, 'lrange').mockRejectedValueOnce(new Error());
      await updateChainTracking({ chain: 'Ethereum', height: 1234567893n });

      const deposit = await getPendingDeposit('Ethereum', 'FLIP', '0x1234');

      expect(deposit).toBeNull();
      expect(logger.error).toHaveBeenCalled();
    });

    it('returns null if the redis client throws (bitcoin)', async () => {
      jest.spyOn(Redis.prototype, 'lrange').mockRejectedValueOnce(new Error());

      const deposit = await getPendingDeposit('Bitcoin', 'BTC', '');

      expect(deposit).toBeNull();
      expect(logger.error).toHaveBeenCalled();
    });
  });

  describe(getPendingBroadcast, () => {
    const broadcast = {
      chain: 'Ethereum',
      nativeId: 1n,
      id: 1234n,
    } as Broadcast;

    it('gets pending broadcasts from redis', async () => {
      await redis.set(
        'broadcast:Ethereum:1',
        JSON.stringify({
          tx_out_id: { signature: { s: [], k_times_g_address: [] } },
        }),
      );

      expect(await getPendingBroadcast(broadcast)).not.toBeNull();
    });

    it('returns null if the broadcast is not found', async () => {
      expect(await getPendingBroadcast(broadcast)).toBeNull();
      expect(logger.error).not.toHaveBeenCalled();
    });

    it('returns null if the client throws an error', async () => {
      jest.spyOn(Redis.prototype, 'get').mockRejectedValueOnce(new Error());

      expect(await getPendingBroadcast(broadcast)).toBeNull();
      expect(logger.error).toHaveBeenCalled();
    });
  });
});
