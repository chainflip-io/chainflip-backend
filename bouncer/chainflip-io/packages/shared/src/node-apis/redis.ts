import { decodeAddress } from '@polkadot/util-crypto';
import Redis from 'ioredis';
import { z } from 'zod';
import { sorter } from '../arrays';
import { type Asset, type Chain } from '../enums';
import { number, u128, string } from '../parsers';

const ss58ToHex = (address: string) =>
  `0x${Buffer.from(decodeAddress(address)).toString('hex')}`;

const jsonString = string.transform((value) => JSON.parse(value));

const depositSchema = jsonString.pipe(
  z.object({
    amount: u128,
    asset: string,
    deposit_chain_block_height: number,
  }),
);

type Deposit = z.infer<typeof depositSchema>;

const sortDepositAscending = sorter<Deposit>('deposit_chain_block_height');

const broadcastParsers = {
  Ethereum: z.object({
    tx_out_id: z.object({
      signature: z.object({
        k_times_g_address: z.array(number),
        s: z.array(number),
      }),
    }),
  }),
  Polkadot: z.object({ tx_out_id: z.object({ signature: string }) }),
  Bitcoin: z.object({ tx_out_id: z.object({ hash: string }) }),
  Arbitrum: z.object({ tx_out_id: z.object({ signature: string }) }),
};

type ChainBroadcast<C extends Chain> = z.infer<(typeof broadcastParsers)[C]>;

type EthereumBroadcast = ChainBroadcast<'Ethereum'>;
type PolkadotBroadcast = ChainBroadcast<'Polkadot'>;
type BitcoinBroadcast = ChainBroadcast<'Bitcoin'>;
type ArbitrumBroadcast = ChainBroadcast<'Arbitrum'>;
type Broadcast = ChainBroadcast<Chain>;

const mempoolTransaction = jsonString.pipe(
  z.object({
    confirmations: number,
    value: u128,
    tx_hash: string.transform((value) => `0x${value}` as const),
  }),
);

export default class RedisClient {
  private client;

  constructor(url: `redis://${string}` | `rediss://${string}`) {
    this.client = new Redis(url);
  }

  async getBroadcast(
    chain: 'Ethereum',
    broadcastId: number | bigint,
  ): Promise<EthereumBroadcast | null>;
  async getBroadcast(
    chain: 'Polkadot',
    broadcastId: number | bigint,
  ): Promise<PolkadotBroadcast | null>;
  async getBroadcast(
    chain: 'Bitcoin',
    broadcastId: number | bigint,
  ): Promise<BitcoinBroadcast | null>;
  async getBroadcast(
    chain: Chain,
    broadcastId: number | bigint,
  ): Promise<Broadcast | null>;
  async getBroadcast(
    chain: Chain,
    broadcastId: number | bigint,
  ): Promise<Broadcast | null> {
    const key = `broadcast:${chain}:${broadcastId}`;
    const value = await this.client.get(key);
    return value ? broadcastParsers[chain].parse(JSON.parse(value)) : null;
  }

  async getDeposits(chain: Chain, asset: Asset, address: string) {
    const parsedAddress = chain === 'Polkadot' ? ss58ToHex(address) : address;
    const key = `deposit:${chain}:${parsedAddress}`;
    const deposits = await this.client.lrange(key, 0, -1);
    return deposits
      .map((deposit) => depositSchema.parse(deposit))
      .filter((deposit) => deposit.asset === asset)
      .sort(sortDepositAscending);
  }

  async getMempoolTransaction(chain: 'Bitcoin', address: string) {
    const key = `mempool:${chain}:${address}`;
    const value = await this.client.get(key);
    return value ? mempoolTransaction.parse(value) : null;
  }

  quit() {
    return this.client.quit();
  }
}
