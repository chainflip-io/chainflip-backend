import { Assets, Chain, Chains } from '@/shared/enums';
import prisma, { SwapDepositChannel } from '../../client';
import { DepositIgnoredArgs } from '../depositIgnored';
import { events } from '../index';
import { SwapAmountTooLowEvent } from '../swapAmountTooLow';
import { SwapExecutedEvent } from '../swapExecuted';
import { SwapScheduledEvent } from '../swapScheduled';

export const ETH_ADDRESS = '0x6Aa69332B63bB5b1d7Ca5355387EDd5624e181F2';
export const DOT_ADDRESS = '1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo'; // 0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972
export const BTC_ADDRESS =
  'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6'; // 0x68a3db628eea903d159131fcb4a1f6ed0be6980c4ff42b80d5229ea26a38439e

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
      brokerCommissionBps: 0,
      expectedDepositAmount: '10000000000',
      issuedBlock: 100,
      estimatedExpiryAt: new Date('2023-11-09T11:05:00.000Z'),
      ...data,
      createdAt: new Date(1690556052834),
    },
  });

const buildSwapScheduledEvent = <T extends SwapScheduledEvent>(args: T) => ({
  block: {
    timestamp: 1670337093000,
    height: 100,
    hash: '0x6c35d3e08b00e979961976cefc79f9594e8ae12f8cc4e9cabfd4796a1994ccd8',
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
      name: events.Swapping.SwapScheduled,
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
        '0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972',
      __kind: 'Dot',
    },
    depositBlockHeight: '100',
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
  swapType: {
    __kind: 'Swap',
  },
});

export const swapScheduledDotDepositChannelBrokerCommissionMock =
  buildSwapScheduledEvent({
    origin: {
      __kind: 'DepositChannel',
      channelId: '2',
      depositAddress: {
        value:
          '0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972',
        __kind: 'Dot',
      },
      depositBlockHeight: '100',
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
    swapType: {
      __kind: 'Swap',
    },
    brokerCommission: 5000000000,
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
    depositBlockHeight: '100',
  },
  swapType: {
    __kind: 'Swap',
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
  swapType: {
    __kind: 'Swap',
  },
});

export const networkDepositReceivedBtcMock = {
  block: {
    height: 120,
    timestamp: 1670337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        asset: {
          __kind: 'Btc',
        },
        amount: '110000',
        depositAddress: {
          value:
            '0x68a3db628eea903d159131fcb4a1f6ed0be6980c4ff42b80d5229ea26a38439e',
          __kind: 'Taproot',
        },
      },
      name: 'BitcoinIngressEgress.DepositReceived',
      indexInBlock: 7,
    },
  },
} as const;

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
      name: events.Swapping.SwapExecuted,
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

export const swapDepositAddressReadyMocked = {
  block: {
    height: 120,
    timestamp: 1670337105000,
    hash: '0x6c35d3e08b00e979961976cefc79f9594e8ae12f8cc4e9cabfd4796a1994ccd8',
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        depositAddress: {
          __kind: 'Eth',
          value: ETH_ADDRESS,
        },
        destinationAddress: {
          __kind: 'Dot',
          value: DOT_ADDRESS,
        },
        sourceAsset: {
          __kind: 'Eth',
        },
        destinationAsset: {
          __kind: 'Dot',
        },
        brokerCommissionRate: 0,
        channelId: '1',
        sourceChainExpiryBlock: '0x100',
      },
      indexInBlock: 0,
      name: events.Swapping.SwapDepositAddressReady,
    },
  },
} as const;

export const swapDepositAddressReadyCcmMetadataMocked = {
  block: {
    height: 120,
    timestamp: 1670337105000,
    hash: '0x6c35d3e08b00e979961976cefc79f9594e8ae12f8cc4e9cabfd4796a1994ccd8',
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        channelId: '8249',
        sourceAsset: { __kind: 'Btc' },
        depositAddress: {
          value:
            '0x7462317079303874383832667679656b63393975336432656a7578347261336c72636b687970776d336137656578363838766a757571687138786e74336b',
          __kind: 'Btc',
        },
        channelMetadata: {
          message: '0xdeadc0de',
          gasBudget: '125000',
          cfParameters: '0x',
        },
        destinationAsset: { __kind: 'Eth' },
        destinationAddress: {
          value: '0xfcd3c82b154cb4717ac98718d0fd13eeba3d2754',
          __kind: 'Eth',
        },
        brokerCommissionRate: 0,
        sourceChainExpiryBlock: '2573643',
      },
      indexInBlock: 0,
      name: events.Swapping.SwapDepositAddressReady,
    },
  },
} as const;

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
      name: events.Swapping.SwapEgressScheduled,
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

export const newPoolCreatedMock = {
  block: {
    height: 120,
    timestamp: 1670337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        baseAsset: { __kind: 'Btc' },
        quoteAsset: { __kind: 'Usdc' },
        initialPrice: '170141183460469231731687303715884105728000',
        feeHundredthPips: 1000,
      },
      name: 'LiquidityPools.NewPoolCreated',
      indexInBlock: 7,
    },
  },
} as const;

export const poolFeeSetMock = {
  block: {
    height: 150,
    timestamp: 1680337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        baseAsset: { __kind: 'Btc' },
        quoteAsset: { __kind: 'Usdc' },
        initialPrice: '170141183460469231731687303715884105728000',
        feeHundredthPips: 2000,
      },
      name: 'LiquidityPools.PoolFeeSet',
      indexInBlock: 7,
    },
  },
} as const;

export const thresholdSignatureInvalidMock = {
  block: {
    height: 420,
    timestamp: 1680337105000,
  },
  eventContext: {
    kind: 'event',
    event: {
      args: {
        broadcastId: 1,
        retryBroadcastId: 10,
      },
      name: 'EthereumBroadcaster.ThresholdSignatureInvalid',
      indexInBlock: 7,
    },
  },
} as const;

const buildSwapAmountTooLowEvent = <T extends SwapAmountTooLowEvent>(
  args: T,
) => ({
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
      name: events.Swapping.SwapAmountTooLow,
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

export const swapAmountTooLowDotDepositChannelMock = buildSwapAmountTooLowEvent(
  {
    amount: '12500000000',
    asset: {
      __kind: 'Dot',
    },
    destinationAddress: {
      value:
        '0x6263727431707a6a64706337393971613566376d36356870723636383830726573356163336c72367932636863346a7361',
      __kind: 'Btc',
    },
    origin: {
      __kind: 'DepositChannel',
      channelId: '2',
      depositAddress: {
        value:
          '0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972',
        __kind: 'Dot',
      },
    },
  },
);

export const swapAmountTooLowBtcDepositChannelMock = buildSwapAmountTooLowEvent(
  {
    amount: '12500000000',
    asset: {
      __kind: 'Btc',
    },
    destinationAddress: {
      value: '0x41ad2bc63a2059f9b623533d87fe99887d794847',
      __kind: 'Eth',
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
  },
);

export const swapAmountTooLowVaultMock = buildSwapAmountTooLowEvent({
  amount: '12500000000',
  asset: {
    __kind: 'Eth',
  },
  destinationAddress: {
    value: '0x2afba9278e30ccf6a6ceb3a8b6e336b70068f045c666f2e7f4f9cc5f47db8972',
    __kind: 'Dot',
  },
  origin: {
    __kind: 'Vault',
    txHash:
      '0x1103ebed92b02a278b54789bfabc056e69ad5c6558049364ea23ec2f3bfa0fd9',
  },
});

export const buildDepositIgnoredEvent = <T extends DepositIgnoredArgs>(
  args: T,
  eventName: string,
) => ({
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
      name: eventName,
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

export const createChainTrackingInfo = () => {
  const chains: Chain[] = ['Bitcoin', 'Ethereum', 'Polkadot'];
  return Promise.all(
    chains.map((chain) =>
      prisma.chainTracking.upsert({
        where: { chain },
        create: {
          chain,
          height: 10,
          blockTrackedAt: new Date('2023-11-09T10:00:00.000Z'),
        },
        update: { height: 10 },
      }),
    ),
  );
};
