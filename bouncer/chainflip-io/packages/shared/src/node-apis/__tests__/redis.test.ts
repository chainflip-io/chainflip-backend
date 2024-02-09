import Redis from 'ioredis';
import RedisClient from '../redis';

jest.mock('ioredis');
const url = 'redis://localhost:6379';

describe(RedisClient, () => {
  describe(RedisClient.prototype.constructor, () => {
    it('creates a new Redis client', () => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const client = new RedisClient(url);
      expect(Redis).toHaveBeenCalledWith(url);
    });
  });

  describe(RedisClient.prototype.getBroadcast, () => {
    it('returns null if the broadcast does not exist', async () => {
      const mock = jest.mocked(Redis.prototype.get).mockResolvedValueOnce(null);
      const client = new RedisClient(url);
      const broadcast = await client.getBroadcast('Ethereum', 1);
      expect(broadcast).toBeNull();
      expect(mock).toHaveBeenCalledWith('broadcast:Ethereum:1');
    });

    it.each([
      ['Bitcoin' as const, { hash: '0x1234' }],
      ['Polkadot' as const, { signature: '0x1234' }],
      ['Ethereum' as const, { signature: { s: [], k_times_g_address: [] } }],
    ])('parses a %s broadcast', async (chain, txOutId) => {
      const mock = jest
        .mocked(Redis.prototype.get)
        .mockResolvedValueOnce(JSON.stringify({ tx_out_id: txOutId }));
      const client = new RedisClient(url);
      const broadcast = await client.getBroadcast(chain, 1);
      expect(broadcast).toMatchSnapshot(`${chain} broadcast`);
      expect(mock).toHaveBeenCalledWith(`broadcast:${chain}:1`);
    });
  });

  describe(RedisClient.prototype.getMempoolTransaction, () => {
    it('returns null if no tx is found for the address', async () => {
      const mock = jest.mocked(Redis.prototype.get).mockResolvedValueOnce(null);
      const client = new RedisClient(url);
      const broadcast = await client.getMempoolTransaction(
        'Bitcoin',
        'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
      );
      expect(broadcast).toBeNull();
      expect(mock).toHaveBeenCalledWith(
        'mempool:Bitcoin:tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
      );
    });

    it('returns the tx if found', async () => {
      const mock = jest.mocked(Redis.prototype.get).mockResolvedValueOnce(
        JSON.stringify({
          confirmations: 4,
          value: '0x12b74280',
          tx_hash: '1234',
        }),
      );
      const client = new RedisClient(url);
      const broadcast = await client.getMempoolTransaction(
        'Bitcoin',
        'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
      );
      expect(broadcast).toEqual({
        confirmations: 4,
        value: 314000000n,
        tx_hash: '0x1234',
      });
      expect(mock).toHaveBeenCalledWith(
        'mempool:Bitcoin:tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
      );
    });
  });

  describe(RedisClient.prototype.getDeposits, () => {
    it('returns an empty array if no deposits are found', async () => {
      const mock = jest
        .mocked(Redis.prototype.lrange)
        .mockResolvedValueOnce([]);
      const client = new RedisClient(url);
      const deposits = await client.getDeposits('Ethereum', 'ETH', '0x1234');
      expect(deposits).toEqual([]);
      expect(mock).toHaveBeenCalledWith('deposit:Ethereum:0x1234', 0, -1);
    });

    it('returns the deposits if found', async () => {
      const mock = jest.mocked(Redis.prototype.lrange).mockResolvedValueOnce([
        JSON.stringify({
          amount: '0x8000',
          asset: 'ETH',
          deposit_chain_block_height: 1234,
        }),
      ]);
      const client = new RedisClient(url);
      const deposits = await client.getDeposits('Ethereum', 'ETH', '0x1234');
      expect(deposits).toEqual([
        {
          amount: 0x8000n,
          asset: 'ETH',
          deposit_chain_block_height: 1234,
        },
      ]);
      expect(mock).toHaveBeenCalledWith('deposit:Ethereum:0x1234', 0, -1);
    });

    it('filters out other assets for the same chain', async () => {
      const mock = jest.mocked(Redis.prototype.lrange).mockResolvedValueOnce([
        JSON.stringify({
          amount: '0x8000',
          asset: 'FLIP',
          deposit_chain_block_height: 1234,
        }),
      ]);
      const client = new RedisClient(url);
      const deposits = await client.getDeposits('Ethereum', 'ETH', '0x1234');
      expect(deposits).toEqual([]);
      expect(mock).toHaveBeenCalledWith('deposit:Ethereum:0x1234', 0, -1);
    });
  });
});
