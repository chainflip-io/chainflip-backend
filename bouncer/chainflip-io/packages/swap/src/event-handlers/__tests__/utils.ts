import { Assets, Chains } from '@/shared/enums';
import prisma, { SwapDepositChannel } from '../../client';
import { swapping } from '../index';
import { SwapExecutedEvent } from '../swapExecuted';
import { SwapScheduledEvent } from '../swapScheduled';

export const ETH_ADDRESS = '0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2';
export const ETH_ADDRESS_2 = '0x6AA69332b63BB5B1d7CA5355387edd5624e181f3';
export const DOT_ADDRESS = '5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX';

type SwapChannelData = Parameters<
  (typeof prisma)['swapDepositChannel']['create']
>[0]['data'];

export const createDepositChannel = (
  data: Partial<SwapChannelData> = {},
): Promise<SwapDepositChannel> =>
  prisma.swapDepositChannel.create({
    data: {
      channelId: 1n,
      srcChain: Chains.Ethereum,
      srcAsset: Assets.ETH,
      destAsset: Assets.DOT,
      depositAddress: ETH_ADDRESS,
      destAddress: DOT_ADDRESS,
      expectedDepositAmount: '10000000000',
      expiryBlock: 200,
      issuedBlock: 100,
      ...data,
    },
  });

const buildSwapScheduledEvent = <T extends SwapScheduledEvent>(args: T) => ({
  block: {
    timestamp: 1670337093000,
    height: 100,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        dispatchInfo: {
          class: [null],
          weight: '101978000',
          paysFee: [null],
        },
        ...args,
      },
      id: '0000012799-000000-c1ea7',
      indexInBlock: 0,
      nodeId: 'WyJldmVudHMiLCIwMDAwMDEyNzk5LTAwMDAwMC1jMWVhNyJd',
      name: swapping.SwapScheduled,
      phase: 'ApplyExtrinsic',
      pos: 2,
      extrinsic: {
        error: null,
        hash: '0xf72d579e0e659b6e287873698da1ffee2f5cbbc1a5165717f0218fca85ba66f4',
        id: '0000012799-000000-c1ea7',
        indexInBlock: 0,
        nodeId: 'WyJleHRyaW5zaWNzIiwiMDAwMDAxMjc5OS0wMDAwMDAtYzFlYTciXQ==',
        pos: 1,
        success: true,
        version: 4,
        call: {
          args: [null],
          error: null,
          id: '0000012799-000000-c1ea7',
          name: 'Timestamp.set',
          nodeId: 'WyJjYWxscyIsIjAwMDAwMTI3OTktMDAwMDAwLWMxZWE3Il0=',
          origin: [null],
          pos: 0,
          success: true,
        },
      },
    },
  },
});

export const swapScheduledDotDepositChannelMock = buildSwapScheduledEvent({
  origin: {
    __kind: 'DepositChannel',
    channelId: '2',
    depositAddress: {
      value:
        '0x08e03063439bf8a21add4a0648439d2095a6e5d88f5ee7ab8fa715b39ef68126',
      __kind: 'Dot',
    },
  },
  swapId: '1',
  sourceAsset: { __kind: 'Dot' },
  depositAmount: '125000000000',
  destinationAsset: { __kind: 'Btc' },
  destinationAddress: {
    value:
      '0x6263727431707a6a64706337393971613566376d36356870723636383830726573356163336c72367932636863346a7361',
    __kind: 'Btc',
  },
});

export const swapScheduledBtcDepositChannelMock = buildSwapScheduledEvent({
  swapId: '3',
  sourceAsset: { __kind: 'Btc' },
  depositAmount: '75000000',
  destinationAsset: { __kind: 'Eth' },
  destinationAddress: {
    __kind: 'Eth',
    value: '0x41ad2bc63a2059f9b623533d87fe99887d794847',
  },
  origin: {
    __kind: 'DepositChannel',
    channelId: '2',
    depositAddress: {
      __kind: 'Btc',
      value:
        '0x6263727431707a6a64706337393971613566376d36356870723636383830726573356163336c72367932636863346a7361',
    },
  },
});

export const swapScheduledVaultMock = buildSwapScheduledEvent({
  origin: {
    __kind: 'Vault',
    txHash:
      '0x1103ebed92b02a278b54789bfabc056e69ad5c6558049364ea23ec2f3bfa0fd9',
  },
  swapId: '2',
  sourceAsset: { __kind: 'Eth' },
  depositAmount: '175000000000000000',
  destinationAsset: { __kind: 'Dot' },
  destinationAddress: {
    value: '0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972',
    __kind: 'Dot',
  },
});

export const buildSwapExecutedMock = (args: SwapExecutedEvent) => ({
  block: {
    height: 100,
    timestamp: 1670337099000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        dispatchInfo: {
          class: [null],
          weight: '101978000',
          paysFee: [null],
        },
        ...args,
      },
      id: '0000012799-000000-c1ea7',
      indexInBlock: 0,
      nodeId: 'WyJldmVudHMiLCIwMDAwMDEyNzk5LTAwMDAwMC1jMWVhNyJd',
      name: swapping.SwapExecuted,
      phase: 'ApplyExtrinsic',
      pos: 2,
      extrinsic: {
        error: null,
        hash: '0xf72d579e0e659b6e287873698da1ffee2f5cbbc1a5165717f0218fca85ba66f4',
        id: '0000012799-000000-c1ea7',
        indexInBlock: 0,
        nodeId: 'WyJleHRyaW5zaWNzIiwiMDAwMDAxMjc5OS0wMDAwMDAtYzFlYTciXQ==',
        pos: 1,
        success: true,
        version: 4,
        call: {
          args: [null],
          error: null,
          id: '0000012799-000000-c1ea7',
          name: 'Timestamp.set',
          nodeId: 'WyJjYWxscyIsIjAwMDAwMTI3OTktMDAwMDAwLWMxZWE3Il0=',
          origin: [null],
          pos: 0,
          success: true,
        },
      },
    },
  },
});

export const swapEgressScheduledMock = {
  block: {
    height: 120,
    timestamp: 1670337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        dispatchInfo: {
          class: [null],
          weight: '101978000',
          paysFee: [null],
        },
        swapId: '9876545',
        egressId: [{ __kind: 'Ethereum' }, '1'] as const,
      },
      id: '0000012799-000000-c1ea7',
      indexInBlock: 0,
      nodeId: 'WyJldmVudHMiLCIwMDAwMDEyNzk5LTAwMDAwMC1jMWVhNyJd',
      name: swapping.SwapEgressScheduled,
      phase: 'ApplyExtrinsic',
      pos: 2,
      extrinsic: {
        error: null,
        hash: '0xf72d579e0e659b6e287873698da1ffee2f5cbbc1a5165717f0218fca85ba66f4',
        id: '0000012799-000000-c1ea7',
        indexInBlock: 0,
        nodeId: 'WyJleHRyaW5zaWNzIiwiMDAwMDAxMjc5OS0wMDAwMDAtYzFlYTciXQ==',
        pos: 1,
        success: true,
        version: 4,
        call: {
          args: [null],
          error: null,
          id: '0000012799-000000-c1ea7',
          name: 'Timestamp.set',
          nodeId: 'WyJjYWxscyIsIjAwMDAwMTI3OTktMDAwMDAwLWMxZWE3Il0=',
          origin: [null],
          pos: 0,
          success: true,
        },
      },
    },
  },
} as const;

export const networkEgressScheduledMock = {
  block: {
    height: 120,
    timestamp: 1670337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        id: [
          {
            __kind: 'Ethereum',
          },
          '13',
        ],
        asset: {
          __kind: 'Usdc',
        },
        amount: '4396575964',
        destinationAddress: '0xa51c1fc2f0d1a1b8494ed1fe312d7c3a78ed91c0',
      },
      name: 'EthereumIngressEgress.EgressScheduled',
      indexInBlock: 123,
    },
  },
} as const;

export const networkBatchBroadcastRequestedMock = {
  block: {
    height: 120,
    timestamp: 1670337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        egressIds: [
          [
            {
              __kind: 'Ethereum',
            },
            '10',
          ],
          [
            {
              __kind: 'Ethereum',
            },
            '11',
          ],
          [
            {
              __kind: 'Ethereum',
            },
            '12',
          ],
        ],
        broadcastId: 9,
      },
      name: 'EthereumIngressEgress.BatchBroadcastRequested',
      indexInBlock: 135,
    },
  },
} as const;

export const networkBroadcastSuccessMock = {
  block: {
    height: 120,
    timestamp: 1670337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        broadcastId: 12,
        transactionOutId: {
          s: '0x689c4add3e14ea8243a1966fc2cea3baea692ca52fd7ef464e1cc74e608bf262',
          kTimesGAddress: '0x972c9f07cc7a847b29003655faf265c12e193f09',
        },
      },
      name: 'EthereumBroadcaster.BroadcastSuccess',
      indexInBlock: 12,
    },
  },
} as const;

export const networkBroadcastAbortedMock = {
  block: {
    height: 120,
    timestamp: 1670337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: { broadcastId: 62 },
      name: 'EthereumBroadcaster.BroadcastAborted',
      indexInBlock: 7,
    },
  },
} as const;
