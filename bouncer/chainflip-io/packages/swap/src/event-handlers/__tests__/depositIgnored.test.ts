import { u8aToHex } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import {
  BTC_ADDRESS,
  DOT_ADDRESS,
  ETH_ADDRESS,
  buildDepositIgnoredEvent,
  createDepositChannel,
} from './utils';
import { events } from '..';
import prisma from '../../client';
import depositIgnored from '../depositIgnored';

const ethDepositIgnoredMock = buildDepositIgnoredEvent(
  {
    asset: { __kind: 'Eth' },
    amount: '100000000000000',
    depositAddress: ETH_ADDRESS,
  },
  events.EthereumIngressEgress.DepositIgnored,
);
const dotDepositIgnoredMock = buildDepositIgnoredEvent(
  {
    asset: { __kind: 'Dot' },
    amount: '1000000000',
    depositAddress: u8aToHex(decodeAddress(DOT_ADDRESS)),
  },
  events.PolkadotIngressEgress.DepositIgnored,
);
const btcDepositIgnoredMock = buildDepositIgnoredEvent(
  {
    asset: { __kind: 'Btc' },
    amount: '100000000000',
    depositAddress: {
      __kind: 'Taproot',
      value:
        '0x68a3db628eea903d159131fcb4a1f6ed0be6980c4ff42b80d5229ea26a38439e',
    },
  },
  events.PolkadotIngressEgress.DepositIgnored,
);

describe(depositIgnored, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel", "Swap" CASCADE`;
    await prisma.$queryRaw`TRUNCATE TABLE "FailedSwap", "Swap" CASCADE`;
  });

  afterEach(async () => {
    jest.resetAllMocks();
  });

  it('handles ignored eth deposits', async () => {
    const channel = await createDepositChannel({
      id: 100n,
      srcChain: 'Ethereum',
      depositAddress: ETH_ADDRESS,
      channelId: 99n,
      destAsset: 'DOT',
      destAddress: DOT_ADDRESS,
    });

    prisma.swapDepositChannel.findFirstOrThrow = jest
      .fn()
      .mockResolvedValueOnce(channel);
    prisma.failedSwap.create = jest.fn();

    await depositIgnored('Ethereum')({
      prisma,
      block: ethDepositIgnoredMock.block as any,
      event: ethDepositIgnoredMock.eventContext.event as any,
    });

    expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenCalledTimes(1);
    expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenNthCalledWith(
      1,
      {
        where: {
          srcChain: 'Ethereum',
          depositAddress: ETH_ADDRESS,
        },
        orderBy: { issuedBlock: 'desc' },
      },
    );
    expect(prisma.failedSwap.create).toHaveBeenCalledTimes(1);
    expect(prisma.failedSwap.create).toHaveBeenNthCalledWith(1, {
      data: {
        destAddress: DOT_ADDRESS,
        destChain: 'Polkadot',
        depositAmount: ethDepositIgnoredMock.eventContext.event.args.amount,
        srcChain: 'Ethereum',
        swapDepositChannelId: 100n,
        type: 'IGNORED',
        reason: 'BelowMinimumDeposit',
      },
    });
  });

  it('handles ignored dot deposits', async () => {
    const channel = await createDepositChannel({
      id: 100n,
      srcChain: 'Polkadot',
      depositAddress: DOT_ADDRESS,
      channelId: 99n,
      destAsset: 'ETH',
      destAddress: ETH_ADDRESS,
    });

    prisma.swapDepositChannel.findFirstOrThrow = jest
      .fn()
      .mockResolvedValueOnce(channel);
    prisma.failedSwap.create = jest.fn();

    await depositIgnored('Polkadot')({
      prisma,
      block: dotDepositIgnoredMock.block as any,
      event: dotDepositIgnoredMock.eventContext.event as any,
    });

    expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenCalledTimes(1);
    expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenNthCalledWith(
      1,
      {
        where: {
          srcChain: 'Polkadot',
          depositAddress: DOT_ADDRESS,
        },
        orderBy: { issuedBlock: 'desc' },
      },
    );
    expect(prisma.failedSwap.create).toHaveBeenCalledTimes(1);
    expect(prisma.failedSwap.create).toHaveBeenNthCalledWith(1, {
      data: {
        destAddress: ETH_ADDRESS,
        destChain: 'Ethereum',
        depositAmount: dotDepositIgnoredMock.eventContext.event.args.amount,
        srcChain: 'Polkadot',
        swapDepositChannelId: 100n,
        type: 'IGNORED',
        reason: 'BelowMinimumDeposit',
      },
    });
  });

  it('handles ignored btc deposits', async () => {
    const channel = await createDepositChannel({
      id: 100n,
      srcChain: 'Bitcoin',
      depositAddress: BTC_ADDRESS,
      channelId: 99n,
      destAsset: 'ETH',
      destAddress: ETH_ADDRESS,
    });

    prisma.swapDepositChannel.findFirstOrThrow = jest
      .fn()
      .mockResolvedValueOnce(channel);
    prisma.failedSwap.create = jest.fn();

    await depositIgnored('Bitcoin')({
      prisma,
      block: btcDepositIgnoredMock.block as any,
      event: btcDepositIgnoredMock.eventContext.event as any,
    });

    expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenCalledTimes(1);
    expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenNthCalledWith(
      1,
      {
        where: {
          srcChain: 'Bitcoin',
          depositAddress: BTC_ADDRESS,
        },
        orderBy: { issuedBlock: 'desc' },
      },
    );
    expect(prisma.failedSwap.create).toHaveBeenCalledTimes(1);
    expect(prisma.failedSwap.create).toHaveBeenNthCalledWith(1, {
      data: {
        destAddress: ETH_ADDRESS,
        destChain: 'Ethereum',
        depositAmount: btcDepositIgnoredMock.eventContext.event.args.amount,
        srcChain: 'Bitcoin',
        swapDepositChannelId: 100n,
        type: 'IGNORED',
        reason: 'BelowMinimumDeposit',
      },
    });
  });
});
