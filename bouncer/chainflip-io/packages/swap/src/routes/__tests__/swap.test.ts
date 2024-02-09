import axios from 'axios';
import * as crypto from 'crypto';
import { Server } from 'http';
import request from 'supertest';
import * as broker from '@/shared/broker';
import { Assets } from '@/shared/enums';
import { environment } from '@/shared/tests/fixtures';
import prisma from '../../client';
import {
  DOT_ADDRESS,
  ETH_ADDRESS,
  createChainTrackingInfo,
  createDepositChannel,
} from '../../event-handlers/__tests__/utils';
import { getPendingBroadcast } from '../../ingress-egress-tracking';
import app from '../../server';
import { State } from '../swap';

jest.mock('../../utils/screenAddress', () => ({
  __esModule: true,
  default: jest.fn().mockResolvedValue(false),
}));

jest.mock('timers/promises', () => ({
  setTimeout: jest.fn().mockResolvedValue(undefined),
}));

jest.mock('../../ingress-egress-tracking');

const randomId = () => BigInt(crypto.randomInt(1, 100000));

jest.mock('@/shared/broker', () => ({
  requestSwapDepositAddress: jest
    .fn()
    .mockRejectedValue(Error('unhandled mock')),
}));

jest.mock('axios', () => ({
  post: jest.fn(() =>
    Promise.resolve({
      data: environment(),
    }),
  ),
}));

const RECEIVED_TIMESTAMP = 1669907135201;
const RECEIVED_BLOCK_INDEX = `100-3`;

describe('server', () => {
  let server: Server;
  jest.setTimeout(1000);

  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel", "Egress", "Broadcast", "FailedSwap" CASCADE`;
    await prisma.$queryRaw`TRUNCATE TABLE "ChainTracking" CASCADE`;
    server = app.listen(0);
  });

  afterEach((cb) => {
    server.close(cb);
  });

  describe('GET /swaps/:id', () => {
    let nativeId: bigint;

    beforeEach(async () => {
      nativeId = randomId();
      jest
        .useFakeTimers({ doNotFake: ['nextTick', 'setImmediate'] })
        .setSystemTime(new Date('2022-01-01'));
      await createChainTrackingInfo();
    });

    afterEach(() => {
      jest.clearAllTimers();
    });

    it('throws an error if no swap deposit channel is found', async () => {
      const { body, status } = await request(server).get(`/swaps/1`);

      expect(status).toBe(404);
      expect(body).toEqual({ message: 'resource not found' });
    });

    it(`retrieves a swap in ${State.AwaitingDeposit} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        expectedDepositAmount: '25000000000000000000000',
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      expect(body).toMatchInlineSnapshot(`
        {
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "25000000000000000000000",
          "feesPaid": [],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "AWAITING_DEPOSIT",
        }
      `);
    });

    it(`retrieves a swap with a broker commission in ${State.AwaitingDeposit} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        expectedDepositAmount: '25000000000000000000000',
        brokerCommissionBps: 15,
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      expect(body).toMatchInlineSnapshot(`
        {
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositChannelBrokerCommissionBps": 15,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "25000000000000000000000",
          "feesPaid": [],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "AWAITING_DEPOSIT",
        }
      `);
    });

    it(`retrieves a swap with ccm metadata in ${State.AwaitingDeposit} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        expectedDepositAmount: '25000000000000000000000',
        ccmMessage: '0xdeadbeef',
        ccmGasBudget: '100000',
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      expect(body).toMatchInlineSnapshot(`
        {
          "ccmMetadata": {
            "gasBudget": "100000",
            "message": "0xdeadbeef",
          },
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "25000000000000000000000",
          "feesPaid": [],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "AWAITING_DEPOSIT",
        }
      `);
    });

    it(`retrieves a swap in ${State.AwaitingDeposit} status and the channel is expired`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 1,
        isExpired: true,
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      expect(body).toMatchInlineSnapshot(`
        {
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "1",
          "depositChannelOpenedThroughBackend": false,
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [],
          "isDepositChannelExpired": true,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "AWAITING_DEPOSIT",
        }
      `);
    });

    it(`retrieves a swap in ${State.DepositReceived} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositAmount: '10',
            swapInputAmount: '10',
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "ccmDepositReceivedBlockIndex": null,
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "DEPOSIT_RECEIVED",
          "swapExecutedBlockIndex": null,
          "type": "SWAP",
        }
      `);
    });

    it(`retrieves a swap in ${State.SwapExecuted} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapInputAmount: '10',
            intermediateAmount: '20',
            fees: {
              create: [
                {
                  type: 'NETWORK',
                  asset: 'USDC',
                  amount: '10',
                },
                {
                  type: 'LIQUIDITY',
                  asset: 'ETH',
                  amount: '5',
                },
              ],
            },
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "ccmDepositReceivedBlockIndex": null,
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "intermediateAmount": "20",
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "SWAP_EXECUTED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
          "type": "SWAP",
        }
      `);
    });

    it(`retrieves a swap in ${State.EgressScheduled} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapInputAmount: '10',
            fees: {
              create: [
                {
                  type: 'NETWORK',
                  asset: 'USDC',
                  amount: '10',
                },
                {
                  type: 'LIQUIDITY',
                  asset: 'ETH',
                  amount: '5',
                },
              ],
            },
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            egress: {
              create: {
                scheduledAt: new Date(RECEIVED_TIMESTAMP + 12000),
                scheduledBlockIndex: `202-3`,
                amount: (10n ** 18n).toString(),
                chain: 'Ethereum',
                nativeId: 1n,
              },
            },
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "ccmDepositReceivedBlockIndex": null,
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "EGRESS_SCHEDULED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
          "type": "SWAP",
        }
      `);
    });

    it(`retrieves a swap in ${State.Broadcasted} status`, async () => {
      jest.mocked(getPendingBroadcast).mockResolvedValueOnce({
        tx_out_id: { hash: '0xdeadbeef' },
      });

      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapInputAmount: '10',
            fees: {
              create: [
                {
                  type: 'NETWORK',
                  asset: 'USDC',
                  amount: '10',
                },
                {
                  type: 'LIQUIDITY',
                  asset: 'ETH',
                  amount: '5',
                },
              ],
            },
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            egress: {
              create: {
                scheduledAt: new Date(RECEIVED_TIMESTAMP + 12000),
                scheduledBlockIndex: `202-3`,
                amount: (10n ** 18n).toString(),
                chain: 'Ethereum',
                nativeId: 1n,
                broadcast: {
                  create: {
                    chain: 'Ethereum',
                    nativeId: 1n,
                    requestedAt: new Date(RECEIVED_TIMESTAMP + 12000),
                    requestedBlockIndex: `202-4`,
                  },
                },
              },
            },
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "broadcastAbortedBlockIndex": null,
          "broadcastRequestedAt": 1669907147201,
          "broadcastRequestedBlockIndex": "202-4",
          "broadcastSucceededBlockIndex": null,
          "ccmDepositReceivedBlockIndex": null,
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "BROADCASTED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
          "type": "SWAP",
        }
      `);
    });

    it(`retrieves a swap in ${State.BroadcastRequested} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapInputAmount: '10',
            fees: {
              create: [
                {
                  type: 'NETWORK',
                  asset: 'USDC',
                  amount: '10',
                },
                {
                  type: 'LIQUIDITY',
                  asset: 'ETH',
                  amount: '5',
                },
              ],
            },
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            egress: {
              create: {
                scheduledAt: new Date(RECEIVED_TIMESTAMP + 12000),
                scheduledBlockIndex: `202-3`,
                amount: (10n ** 18n).toString(),
                chain: 'Ethereum',
                nativeId: 1n,
                broadcast: {
                  create: {
                    chain: 'Ethereum',
                    nativeId: 1n,
                    requestedAt: new Date(RECEIVED_TIMESTAMP + 12000),
                    requestedBlockIndex: `202-4`,
                  },
                },
              },
            },
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "broadcastAbortedBlockIndex": null,
          "broadcastRequestedAt": 1669907147201,
          "broadcastRequestedBlockIndex": "202-4",
          "broadcastSucceededBlockIndex": null,
          "ccmDepositReceivedBlockIndex": null,
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "BROADCAST_REQUESTED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
          "type": "SWAP",
        }
      `);
    });

    it(`retrieves a swap in ${State.BroadcastAborted} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapInputAmount: '10',
            fees: {
              create: [
                {
                  type: 'NETWORK',
                  asset: 'USDC',
                  amount: '10',
                },
                {
                  type: 'LIQUIDITY',
                  asset: 'ETH',
                  amount: '5',
                },
              ],
            },
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            egress: {
              create: {
                scheduledAt: new Date(RECEIVED_TIMESTAMP + 12000),
                scheduledBlockIndex: `202-3`,
                amount: (10n ** 18n).toString(),
                chain: 'Ethereum',
                nativeId: 2n,
                broadcast: {
                  create: {
                    chain: 'Ethereum',
                    nativeId: 2n,
                    requestedAt: new Date(RECEIVED_TIMESTAMP + 12000),
                    requestedBlockIndex: `202-4`,
                    abortedAt: new Date(RECEIVED_TIMESTAMP + 18000),
                    abortedBlockIndex: `204-4`,
                  },
                },
              },
            },
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "broadcastAbortedAt": 1669907153201,
          "broadcastAbortedBlockIndex": "204-4",
          "broadcastRequestedAt": 1669907147201,
          "broadcastRequestedBlockIndex": "202-4",
          "broadcastSucceededBlockIndex": null,
          "ccmDepositReceivedBlockIndex": null,
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "BROADCAST_ABORTED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
          "type": "SWAP",
        }
      `);
    });

    it(`retrieves a swap in ${State.Complete} status`, async () => {
      const swapIntent = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapInputAmount: '10',
            fees: {
              create: [
                {
                  type: 'NETWORK',
                  asset: 'USDC',
                  amount: '10',
                },
                {
                  type: 'LIQUIDITY',
                  asset: 'ETH',
                  amount: '5',
                },
              ],
            },
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            egress: {
              create: {
                scheduledAt: new Date(RECEIVED_TIMESTAMP + 12000),
                scheduledBlockIndex: `202-3`,
                amount: (10n ** 18n).toString(),
                chain: 'Ethereum',
                nativeId: 3n,
                broadcast: {
                  create: {
                    chain: 'Ethereum',
                    nativeId: 3n,
                    requestedAt: new Date(RECEIVED_TIMESTAMP + 12000),
                    requestedBlockIndex: `202-4`,
                    succeededAt: new Date(RECEIVED_TIMESTAMP + 18000),
                    succeededBlockIndex: `204-4`,
                  },
                },
              },
            },
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "broadcastAbortedBlockIndex": null,
          "broadcastRequestedAt": 1669907147201,
          "broadcastRequestedBlockIndex": "202-4",
          "broadcastSucceededAt": 1669907153201,
          "broadcastSucceededBlockIndex": "204-4",
          "ccmDepositReceivedBlockIndex": null,
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositChannelBrokerCommissionBps": 0,
          "depositChannelCreatedAt": 1690556052834,
          "depositChannelExpiryBlock": "200",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "estimatedDepositChannelExpiryTime": 1699527900000,
          "expectedDepositAmount": "10000000000",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "COMPLETE",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
          "type": "SWAP",
        }
      `);
    });

    it(`retrieves a swap in ${State.Failed} status`, async () => {
      const channel = await createDepositChannel({
        srcChainExpiryBlock: 200,
      });
      await prisma.failedSwap.create({
        data: {
          type: 'IGNORED',
          reason: 'BelowMinimumDeposit',
          swapDepositChannelId: channel.id,
          srcChain: 'Ethereum',
          destAddress: channel.destAddress,
          destChain: 'Polkadot',
          depositAmount: '10000000000',
        },
      });

      const channelId = `${channel.issuedBlock}-${channel.srcChain}-${channel.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);

      expect(body).toMatchObject({
        failed: {
          reason: 'BelowMinimumDeposit',
          type: 'IGNORED',
        },
        state: 'FAILED',
      });
    });

    it(`returns ${State.Failed} status even if it has other completed swaps`, async () => {
      const channel = await createDepositChannel({
        srcChainExpiryBlock: 200,
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapInputAmount: '10',
            fees: {
              create: [
                {
                  type: 'NETWORK',
                  asset: 'USDC',
                  amount: '10',
                },
                {
                  type: 'LIQUIDITY',
                  asset: 'ETH',
                  amount: '5',
                },
              ],
            },
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            egress: {
              create: {
                scheduledAt: new Date(RECEIVED_TIMESTAMP + 12000),
                scheduledBlockIndex: `202-3`,
                amount: (10n ** 18n).toString(),
                chain: 'Ethereum',
                nativeId: 3n,
                broadcast: {
                  create: {
                    chain: 'Ethereum',
                    nativeId: 3n,
                    requestedAt: new Date(RECEIVED_TIMESTAMP + 12000),
                    requestedBlockIndex: `202-4`,
                    succeededAt: new Date(RECEIVED_TIMESTAMP + 18000),
                    succeededBlockIndex: `204-4`,
                  },
                },
              },
            },
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
            type: 'SWAP',
          },
        },
      });
      await prisma.failedSwap.create({
        data: {
          type: 'IGNORED',
          reason: 'BelowMinimumDeposit',
          swapDepositChannelId: channel.id,
          srcChain: 'Ethereum',
          destAddress: channel.destAddress,
          destChain: 'Polkadot',
          depositAmount: '10000000000',
        },
      });

      const channelId = `${channel.issuedBlock}-${channel.srcChain}-${channel.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(rest).toMatchObject({
        state: 'FAILED',
        swapExecutedAt: 1669907141201,
        failed: { type: 'IGNORED', reason: 'BelowMinimumDeposit' },
      });
    });

    it('retrieves a swap from a vault origin', async () => {
      const txHash = `0x${crypto.randomBytes(64).toString('hex')}`;

      await prisma.swap.create({
        data: {
          txHash,
          nativeId,
          srcAsset: Assets.ETH,
          destAsset: Assets.DOT,
          destAddress: DOT_ADDRESS,
          depositAmount: '10',
          swapInputAmount: '10',
          fees: {
            create: [
              {
                type: 'NETWORK',
                asset: 'USDC',
                amount: '10',
              },
              {
                type: 'LIQUIDITY',
                asset: 'ETH',
                amount: '5',
              },
            ],
          },
          depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
          depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
          type: 'SWAP',
        },
      });

      const { body, status } = await request(server).get(`/swaps/${txHash}`);
      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "ccmDepositReceivedBlockIndex": null,
          "depositAmount": "10",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "DEPOSIT_RECEIVED",
          "swapExecutedBlockIndex": null,
          "type": "SWAP",
        }
      `);
    });

    it('retrieves a swap from a native swap id', async () => {
      await prisma.swap.create({
        data: {
          nativeId,
          srcAsset: Assets.ETH,
          destAsset: Assets.DOT,
          destAddress: DOT_ADDRESS,
          depositAmount: '10',
          swapInputAmount: '10',
          fees: {
            create: [
              {
                type: 'NETWORK',
                asset: 'USDC',
                amount: '10',
              },
              {
                type: 'LIQUIDITY',
                asset: 'ETH',
                amount: '5',
              },
            ],
          },
          depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
          depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
          type: 'SWAP',
          ccmDepositReceivedBlockIndex: '223-16',
          ccmGasBudget: '100',
          ccmMessage: '0x12abf87',
        },
      });

      const { body, status } = await request(server).get(`/swaps/${nativeId}`);
      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "ccmDepositReceivedBlockIndex": "223-16",
          "ccmMetadata": {
            "gasBudget": "100",
            "message": "0x12abf87",
          },
          "depositAmount": "10",
          "depositChannelOpenedThroughBackend": false,
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "feesPaid": [
            {
              "amount": "10",
              "asset": "USDC",
              "chain": "Ethereum",
              "type": "NETWORK",
            },
            {
              "amount": "5",
              "asset": "ETH",
              "chain": "Ethereum",
              "type": "LIQUIDITY",
            },
          ],
          "isDepositChannelExpired": false,
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "DEPOSIT_RECEIVED",
          "swapExecutedBlockIndex": null,
          "type": "SWAP",
        }
      `);
    });
  });

  describe('POST /swaps', () => {
    const ethToDotSwapRequestBody = {
      srcAsset: Assets.ETH,
      srcChain: 'Ethereum',
      destAsset: Assets.DOT,
      destChain: 'Polkadot',
      destAddress: DOT_ADDRESS,
      amount: '1000000000',
    } as const;
    const dotToEthSwapRequestBody = {
      srcAsset: Assets.DOT,
      srcChain: 'Polkadot',
      destAsset: Assets.ETH,
      destChain: 'Ethereum',
      destAddress: ETH_ADDRESS,
      amount: '1000000000',
    } as const;

    it.each([
      [ethToDotSwapRequestBody], // a comment to prevent a huge PR diff
      [dotToEthSwapRequestBody],
    ])('creates a new swap deposit channel', async (requestBody) => {
      const issuedBlock = 123;
      const channelId = 200n;
      const address = 'THE_INGRESS_ADDRESS';
      const sourceChainExpiryBlock = 1_000_000n;
      jest.mocked(broker.requestSwapDepositAddress).mockResolvedValueOnce({
        address,
        issuedBlock,
        channelId,
        sourceChainExpiryBlock,
      });

      const { body, status } = await request(app)
        .post('/swaps')
        .send(requestBody);

      expect(body).toMatchObject({
        id: `123-${requestBody.srcChain}-200`,
        depositAddress: address,
        issuedBlock,
      });
      expect(status).toBe(200);

      const swapDepositChannel =
        await prisma.swapDepositChannel.findFirstOrThrow({
          where: { depositAddress: address },
        });

      expect(swapDepositChannel).toMatchObject({
        id: expect.any(BigInt),
        srcAsset: requestBody.srcAsset,
        depositAddress: address,
        destAsset: requestBody.destAsset,
        destAddress: requestBody.destAddress,
        issuedBlock,
        channelId,
        createdAt: expect.any(Date),
      });
      expect(swapDepositChannel?.expectedDepositAmount?.toString()).toBe(
        requestBody.amount,
      );
    });

    it('does not update the already existing deposit channel', async () => {
      const requestBody = ethToDotSwapRequestBody;
      const channelId = 200n;
      const oldAddress = 'THE_INGRESS_ADDRESS';
      const newAddress = 'THE_NEW_INGRESS_ADDRESS';
      const issuedBlock = 123;
      const sourceChainExpiryBlock = 1_000_000n;

      await createDepositChannel({
        channelId,
        srcChain: requestBody.srcChain,
        issuedBlock,
        depositAddress: oldAddress,
      });

      jest.mocked(broker.requestSwapDepositAddress).mockResolvedValueOnce({
        address: newAddress,
        issuedBlock,
        channelId,
        sourceChainExpiryBlock,
      });

      const { body, status } = await request(app)
        .post('/swaps')
        .send(requestBody);

      expect(status).toBe(200);
      expect(body).toMatchObject({
        issuedBlock,
        depositAddress: oldAddress,
      });
    });

    it.each([
      ['srcAsset', 'SHIB'],
      ['destAsset', 'SHIB'],
    ])('rejects an invalid %s', async (key, value) => {
      const requestBody = {
        srcAsset: Assets.ETH,
        destAsset: Assets.DOT,
        destAddress: DOT_ADDRESS,
        expectedDepositAmount: '1000000000',
        [key]: value,
      };

      const { body, status } = await request(app)
        .post('/swaps')
        .send(requestBody);

      expect(status).toBe(400);
      expect(body).toMatchObject({ message: 'invalid request body' });
    });

    it.each([
      {
        srcAsset: Assets.DOT,
        destAsset: Assets.ETH,
        srcChain: 'Polkadot',
        destChain: 'Ethereum',
        destAddress: '0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181f2',
        amount: '1000000000',
      },
      {
        srcAsset: Assets.ETH,
        destAsset: Assets.DOT,
        srcChain: 'Ethereum',
        destChain: 'Polkadot',
        destAddress: '0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181f2',
        amount: '1000000000',
      },
    ])('throws on bad addresses (%s)', async (requestBody) => {
      const { body, status } = await request(app)
        .post('/swaps')
        .send(requestBody);

      expect(status).toBe(400);
      expect(body).toMatchObject({
        message: 'provided address is not valid',
      });
    });

    it('rejects if amount is lower than minimum swap amount', async () => {
      jest.mocked(axios.post).mockResolvedValueOnce({
        data: environment({ minDepositAmount: '0xffffff' }),
      });

      const { body, status } = await request(app).post('/swaps').send({
        srcAsset: Assets.DOT,
        destAsset: Assets.ETH,
        srcChain: 'Polkadot',
        destChain: 'Ethereum',
        destAddress: ETH_ADDRESS,
        amount: '5',
      });

      expect(status).toBe(400);
      expect(body).toMatchObject({
        message: 'expected amount is below minimum swap amount (16777215)',
      });
    });

    it('rejects if amount is higher than maximum swap amount', async () => {
      jest
        .mocked(axios.post)
        .mockResolvedValueOnce({ data: environment({ maxSwapAmount: '0x1' }) });

      const { body, status } = await request(app).post('/swaps').send({
        srcAsset: Assets.USDC,
        destAsset: Assets.ETH,
        srcChain: 'Ethereum',
        destChain: 'Ethereum',
        destAddress: ETH_ADDRESS,
        amount: '5',
      });

      expect(status).toBe(400);
      expect(body).toMatchObject({
        message: 'expected amount is above maximum swap amount (1)',
      });
    });
  });
});
