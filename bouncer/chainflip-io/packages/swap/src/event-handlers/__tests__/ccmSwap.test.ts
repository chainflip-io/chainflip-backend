/* eslint-disable @typescript-eslint/no-empty-function */
import { GraphQLClient } from 'graphql-request';
import { assetChains } from '@/shared/enums';
import { environment } from '@/shared/tests/fixtures';
import prisma from '../../client';
import { Event } from '../../gql/generated/graphql';
import processBlocks from '../../processBlocks';
import { encodedAddress } from '../common';
import { SwapScheduledEvent } from '../swapScheduled';

jest.mock('graphql-request', () => ({
  GraphQLClient: class MockClient {
    request() {}
  },
}));

jest.mock('axios', () => ({
  post: jest.fn(() =>
    Promise.resolve({ data: environment({ egressFee: '0x55524' }) }),
  ),
}));

const uppercase = <const T extends string>(str: T): Uppercase<T> =>
  str.toUpperCase() as Uppercase<T>;

const swapDepositAddressReadyEvent = {
  id: '0000000001-000057-1d5a7',
  blockId: '0000000001-1d5a7',
  indexInBlock: 57,
  extrinsicId: '0000000001-000016-1d5a7',
  callId: '0000000001-000016-1d5a7',
  name: 'Swapping.SwapDepositAddressReady',
  args: {
    channelId: '6',
    sourceAsset: { __kind: 'Dot' },
    depositAddress: {
      value:
        '0xf1ebeed3a1b2bd9a24643e26509d52505d8c12a2d667ab8f66255f4c51ba0dbe',
      __kind: 'Dot',
    },
    destinationAsset: { __kind: 'Eth' },
    destinationAddress: {
      value: '0xa51c1fc2f0d1a1b8494ed1fe312d7c3a78ed91c0',
      __kind: 'Eth',
    },
    sourceChainExpiryBlock: '0x100',
  },
} as const;

const ccmEvents = [
  // swapDepositAddressReadyEvent,
  {
    id: '0000000002-000013-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 13,
    extrinsicId: '0000000002-000006-a6740',
    callId: '0000000002-000006-a6740',
    name: 'Swapping.SwapScheduled',
    args: {
      origin: {
        __kind: 'DepositChannel',
        channelId: '6',
        depositAddress: {
          value:
            '0xf1ebeed3a1b2bd9a24643e26509d52505d8c12a2d667ab8f66255f4c51ba0dbe',
          __kind: 'Dot',
        },
        depositBlockHeight: '100',
      },
      swapId: '1',
      swapType: { value: '1', __kind: 'CcmPrincipal' },
      sourceAsset: { __kind: 'Dot' },
      depositAmount: '499999000000',
      destinationAsset: { __kind: 'Eth' },
      destinationAddress: {
        value: '0xa51c1fc2f0d1a1b8494ed1fe312d7c3a78ed91c0',
        __kind: 'Eth',
      },
    } as SwapScheduledEvent,
  },
  {
    id: '0000000002-000014-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 14,
    extrinsicId: '0000000002-000006-a6740',
    callId: '0000000002-000006-a6740',
    name: 'Swapping.SwapScheduled',
    args: {
      origin: {
        __kind: 'DepositChannel',
        channelId: '6',
        depositAddress: {
          value:
            '0xf1ebeed3a1b2bd9a24643e26509d52505d8c12a2d667ab8f66255f4c51ba0dbe',
          __kind: 'Dot',
        },
        depositBlockHeight: '100',
      },
      swapId: '2',
      swapType: { value: '1', __kind: 'CcmGas' },
      sourceAsset: { __kind: 'Dot' },
      depositAmount: '1000000',
      destinationAsset: { __kind: 'Eth' },
      destinationAddress: {
        value: '0xa51c1fc2f0d1a1b8494ed1fe312d7c3a78ed91c0',
        __kind: 'Eth',
      },
    } as SwapScheduledEvent,
  },
  {
    id: '0000000002-000054-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 54,
    extrinsicId: null,
    callId: null,
    name: 'Swapping.SwapExecuted',
    args: {
      swapId: '1',
      sourceAsset: { __kind: 'Dot' },
      egressAmount: '480766415032706356',
      depositAmount: '499999000000',
      destinationAsset: { __kind: 'Eth' },
      intermediateAmount: '485434508',
    },
  },
  {
    id: '0000000002-000055-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 55,
    extrinsicId: null,
    callId: null,
    name: 'Swapping.SwapExecuted',
    args: {
      swapId: '2',
      sourceAsset: { __kind: 'Dot' },
      egressAmount: '960672170800',
      depositAmount: '1000000',
      destinationAsset: { __kind: 'Eth' },
      intermediateAmount: '970',
    },
  },
  {
    id: '0000000002-000056-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 56,
    extrinsicId: null,
    callId: null,
    name: 'EthereumIngressEgress.EgressScheduled',
    args: {
      id: [{ __kind: 'Ethereum' }, '1'],
      asset: { __kind: 'Eth' },
      amount: '480766415032706356',
      destinationAddress: '0xa51c1fc2f0d1a1b8494ed1fe312d7c3a78ed91c0',
    },
  },
  {
    id: '0000000002-000057-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 57,
    extrinsicId: null,
    callId: null,
    name: 'Swapping.SwapEgressScheduled',
    args: {
      asset: { __kind: 'Eth' },
      amount: '480766415032706356',
      swapId: '1',
      egressId: [{ __kind: 'Ethereum' }, '1'],
    },
  },
  {
    id: '0000000002-000058-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 58,
    extrinsicId: null,
    callId: null,
    name: 'Swapping.SwapEgressScheduled',
    args: {
      asset: { __kind: 'Eth' },
      amount: '960672170800',
      swapId: '2',
      egressId: [{ __kind: 'Ethereum' }, '1'],
    },
  },
  {
    id: '0000000002-000014-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 18,
    extrinsicId: '0000000002-000006-a6740',
    callId: '0000000002-000006-a6740',
    name: 'Swapping.CcmDepositReceived',
    args: {
      ccmId: '123',
      principalSwapId: '2',
      depositAmount: '1000000',
      destinationAddress: {
        value: '0xa51c1fc2f0d1a1b8494ed1fe312d7c3a78ed91c0',
        __kind: 'Eth',
      },
      depositMetadata: {
        channelMetadata: {
          message: '0x12abf87',
          gasBudget: '2000',
        },
      },
    },
  },
  {
    id: '0000000002-000081-a6740',
    blockId: '0000000002-a6740',
    indexInBlock: 81,
    extrinsicId: null,
    callId: null,
    name: 'EthereumIngressEgress.CcmBroadcastRequested',
    args: { egressId: [{ __kind: 'Ethereum' }, '1'], broadcastId: 4 },
  },
  {
    id: '0000000003-000025-c70a8',
    blockId: '0000000003-c70a8',
    indexInBlock: 25,
    extrinsicId: '0000000003-000011-c70a8',
    callId: '0000000003-000011-c70a8',
    name: 'EthereumBroadcaster.BroadcastSuccess',
    args: {
      broadcastId: 4,
      transactionOutId: {
        s: '0x015d2f27f9f6e27115233a984614ad20ea471862491e5c5fd953c8da124171fe',
        kTimesGAddress: '0x56302c8059aa77b84842cff40cca72ec6db3c522',
      },
    },
  },
]
  .sort((a, b) => (a.id < b.id ? -1 : 1))
  .reduce((acc, event) => {
    acc.set(
      event.blockId,
      (acc.get(event.blockId) || []).concat([event as Event]),
    );
    return acc;
  }, new Map<string, Event[]>());

describe('batch swap flow', () => {
  beforeAll(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE public."Pool" CASCADE`;
    await prisma.pool.createMany({
      data: [
        {
          baseAsset: 'ETH',
          quoteAsset: 'USDC',
          liquidityFeeHundredthPips: 1000,
        },

        {
          baseAsset: 'DOT',
          quoteAsset: 'USDC',
          liquidityFeeHundredthPips: 1500,
        },
      ],
    });
  });

  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "Egress", "Broadcast", "Swap", "SwapDepositChannel" CASCADE`;
  });

  it('handles all the events', async () => {
    const startingHeight =
      Number(ccmEvents.keys().next().value.split('-')[0]) - 1;
    await prisma.state.upsert({
      where: { id: 1 },
      create: { id: 1, height: startingHeight },
      update: { height: startingHeight },
    });

    const blocksIt = ccmEvents.entries();

    await prisma.swapDepositChannel.create({
      data: {
        srcAsset: uppercase(
          swapDepositAddressReadyEvent.args.sourceAsset.__kind,
        ),
        depositAddress: encodedAddress.parse(
          swapDepositAddressReadyEvent.args.depositAddress,
        ).address,
        srcChain:
          assetChains[
            uppercase(swapDepositAddressReadyEvent.args.sourceAsset.__kind)
          ],
        channelId: BigInt(swapDepositAddressReadyEvent.args.channelId),
        expectedDepositAmount: '0',
        destAsset: uppercase(
          swapDepositAddressReadyEvent.args.destinationAsset.__kind,
        ),
        destAddress: swapDepositAddressReadyEvent.args.destinationAddress.value,
        brokerCommissionBps: 0,
        issuedBlock: 0,
        srcChainExpiryBlock: Number(
          swapDepositAddressReadyEvent.args.sourceChainExpiryBlock,
        ),
      },
    });

    jest
      .spyOn(GraphQLClient.prototype, 'request')
      .mockImplementation(async () => {
        const batch = blocksIt.next();
        if (batch.done) throw new Error('done');
        const [blockId, events] = batch.value;
        const height = Number(blockId.split('-')[0]);
        await prisma.state.upsert({
          where: { id: 1 },
          create: { id: 1, height: height - 1 },
          update: { height: height - 1 },
        });

        return {
          blocks: {
            nodes: [
              {
                height,
                specId: 'test@0',
                timestamp: new Date(height * 6000).toISOString(),
                events: { nodes: events },
              },
            ],
          },
        };
      });

    await expect(processBlocks()).rejects.toThrow('done');

    const swaps = await prisma.swap.findMany();

    expect(swaps).toHaveLength(2);

    expect(swaps[0]).toMatchSnapshot(
      {
        id: expect.any(BigInt),
        swapDepositChannelId: expect.any(BigInt),
        egressId: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
      },
      'principal swap',
    );
    expect(swaps[1]).toMatchSnapshot(
      {
        id: expect.any(BigInt),
        swapDepositChannelId: expect.any(BigInt),
        egressId: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
      },
      'gas swap',
    );

    const egresses = await prisma.egress.findMany();
    expect(egresses).toHaveLength(1);
    expect(egresses[0]).toMatchSnapshot(
      {
        id: expect.any(BigInt),
        broadcastId: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
      },
      'egress',
    );

    const broadcasts = await prisma.broadcast.findMany();
    expect(broadcasts).toHaveLength(1);
    expect(broadcasts[0]).toMatchSnapshot(
      {
        id: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
      },
      'broadcast',
    );
  });
});
