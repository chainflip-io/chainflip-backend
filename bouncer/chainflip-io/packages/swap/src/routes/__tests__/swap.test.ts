import * as crypto from 'crypto';
import { Server } from 'http';
import request from 'supertest';
import { Assets } from '@/shared/enums';
import RpcClient from '@/shared/node-apis/RpcClient';
import prisma from '../../client';
import {
  DOT_ADDRESS,
  ETH_ADDRESS,
  createDepositChannel,
} from '../../event-handlers/__tests__/utils';
import app from '../../server';
import { State } from '../swap';

jest.mock('timers/promises', () => ({
  setTimeout: jest.fn().mockResolvedValue(undefined),
}));

const randomId = () => BigInt(crypto.randomInt(1, 100000));

jest.mock(
  '@/shared/node-apis/RpcClient',
  () =>
    class {
      async connect() {
        return this;
      }

      sendRequest() {
        throw new Error('unmocked request');
      }
    },
);

const HEX_DOT_ADDRESS = '0xca';
const RECEIVED_TIMESTAMP = 1669907135201;
const RECEIVED_BLOCK_INDEX = `100-3`;

describe('server', () => {
  let server: Server;
  jest.setTimeout(1000);

  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel" CASCADE`;
    server = app.listen(0);
  });

  afterEach((cb) => {
    server.close(cb);
  });

  describe('GET /swaps/:id', () => {
    let nativeId: bigint;

    beforeEach(() => {
      nativeId = randomId();
    });

    it('throws an error if no swap deposit channel is found', async () => {
      const { body, status } = await request(server).get(`/swaps/1`);

      expect(status).toBe(404);
      expect(body).toEqual({ message: 'resource not found' });
    });

    it(`retrieves a swap in ${State.AwaitingDeposit} status`, async () => {
      const swapIntent = await createDepositChannel();
      const channelId = `${swapIntent.issuedBlock}-${swapIntent.srcChain}-${swapIntent.channelId}`;

      const { body, status } = await request(server).get(`/swaps/${channelId}`);

      expect(status).toBe(200);
      expect(body).toMatchInlineSnapshot(`
        {
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "expectedDepositAmount": "10000000000",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "AWAITING_DEPOSIT",
        }
      `);
    });

    it(`retrieves a swap in ${State.DepositReceived} status`, async () => {
      const swapIntent = await createDepositChannel({
        swaps: {
          create: {
            nativeId,
            depositAmount: '10',
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
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
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "expectedDepositAmount": "10000000000",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "DEPOSIT_RECEIVED",
          "swapExecutedBlockIndex": null,
        }
      `);
    });

    it(`retrieves a swap in ${State.SwapExecuted} status`, async () => {
      const swapIntent = await createDepositChannel({
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
            swapExecutedAt: new Date(RECEIVED_TIMESTAMP + 6000),
            swapExecutedBlockIndex: `200-3`,
            srcAsset: Assets.ETH,
            destAsset: Assets.DOT,
            destAddress: DOT_ADDRESS,
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
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "expectedDepositAmount": "10000000000",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "SWAP_EXECUTED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
        }
      `);
    });

    it(`retrieves a swap in ${State.EgressScheduled} status`, async () => {
      const swapIntent = await createDepositChannel({
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
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
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "expectedDepositAmount": "10000000000",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "EGRESS_SCHEDULED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
        }
      `);
    });

    it(`retrieves a swap in ${State.BroadcastRequested} status`, async () => {
      const swapIntent = await createDepositChannel({
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
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
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "expectedDepositAmount": "10000000000",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "BROADCAST_REQUESTED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
        }
      `);
    });

    it(`retrieves a swap in ${State.BroadcastAborted} status`, async () => {
      const swapIntent = await createDepositChannel({
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
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
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "expectedDepositAmount": "10000000000",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "BROADCAST_ABORTED",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
        }
      `);
    });

    it(`retrieves a swap in ${State.Complete} status`, async () => {
      const swapIntent = await createDepositChannel({
        swaps: {
          create: {
            nativeId,
            depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
            depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
            depositAmount: '10',
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
          "depositAddress": "0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2",
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "egressAmount": "1000000000000000000",
          "egressScheduledAt": 1669907147201,
          "egressScheduledBlockIndex": "202-3",
          "expectedDepositAmount": "10000000000",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "COMPLETE",
          "swapExecutedAt": 1669907141201,
          "swapExecutedBlockIndex": "200-3",
        }
      `);
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
          depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
          depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
        },
      });

      const { body, status } = await request(server).get(`/swaps/${txHash}`);
      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "DEPOSIT_RECEIVED",
          "swapExecutedBlockIndex": null,
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
          depositReceivedAt: new Date(RECEIVED_TIMESTAMP),
          depositReceivedBlockIndex: RECEIVED_BLOCK_INDEX,
        },
      });

      const { body, status } = await request(server).get(`/swaps/${nativeId}`);
      expect(status).toBe(200);
      const { swapId, ...rest } = body;
      expect(BigInt(swapId)).toEqual(nativeId);
      expect(rest).toMatchInlineSnapshot(`
        {
          "depositAmount": "10",
          "depositReceivedAt": 1669907135201,
          "depositReceivedBlockIndex": "100-3",
          "destAddress": "5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX",
          "destAsset": "DOT",
          "destChain": "Polkadot",
          "srcAsset": "ETH",
          "srcChain": "Ethereum",
          "state": "DEPOSIT_RECEIVED",
          "swapExecutedBlockIndex": null,
        }
      `);
    });
  });

  describe('POST /swaps', () => {
    it.each([
      [
        {
          srcAsset: Assets.ETH,
          destAsset: Assets.DOT,
          srcChain: 'Ethereum',
          destChain: 'Polkadot',
          destAddress: HEX_DOT_ADDRESS,
          amount: '1000000000',
        },
      ],
      [
        {
          srcAsset: Assets.ETH,
          destAsset: Assets.DOT,
          srcChain: 'Ethereum',
          destChain: 'Polkadot',
          destAddress: DOT_ADDRESS,
          amount: '1000000000',
        },
      ],
      [
        {
          srcAsset: Assets.DOT,
          destAsset: Assets.ETH,
          srcChain: 'Ethereum',
          destChain: 'Polkadot',
          destAddress: ETH_ADDRESS,
          amount: '1000000000',
        },
      ],
    ])('creates a new swap deposit channel', async (requestBody) => {
      const issuedBlock = 123;
      const expiryBlock = 200;
      const channelId = 200n;
      const address = 'THE_INGRESS_ADDRESS';
      jest.spyOn(RpcClient.prototype, 'sendRequest').mockResolvedValueOnce({
        address,
        expiryBlock,
        issuedBlock,
        channelId,
      });

      const { body, status } = await request(app)
        .post('/swaps')
        .send(requestBody);

      const swapDepositChannel = await prisma.swapDepositChannel.findFirst({
        where: { depositAddress: address },
      });

      expect(swapDepositChannel).toMatchObject({
        id: expect.any(BigInt),
        srcAsset: requestBody.srcAsset,
        depositAddress: address,
        destAsset: requestBody.destAsset,
        destAddress: requestBody.destAddress,
        expiryBlock,
        issuedBlock,
        channelId,
        createdAt: expect.any(Date),
      });
      expect(swapDepositChannel?.expectedDepositAmount.toString()).toBe(
        requestBody.amount,
      );
      expect(status).toBe(200);
      expect(body).toMatchObject({
        id: '123-Ethereum-200',
        depositAddress: address,
        issuedBlock,
        expiryBlock,
      });
    });

    it.each([
      ['srcAsset', 'SHIB'],
      ['destAsset', 'SHIB'],
    ])('rejects an invalid %s', async (key, value) => {
      const requestBody = {
        srcAsset: Assets.ETH,
        destAsset: Assets.DOT,
        destAddress: HEX_DOT_ADDRESS,
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
      expect(body).toMatchObject({ message: 'provided address is not valid' });
    });
  });
});
