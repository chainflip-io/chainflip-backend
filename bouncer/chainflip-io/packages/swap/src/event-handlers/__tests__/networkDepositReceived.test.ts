import { networkDepositReceivedBtcMock } from '@/swap/event-handlers/__tests__/utils';
import prisma from '../../client';
import {
  depositReceivedArgs,
  networkDepositReceived,
} from '../networkDepositReceived';

describe('depositReceived', () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "private"."DepositChannel" CASCADE`;
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel", "Swap" CASCADE`;
  });

  it('should update the values for an existing swap', async () => {
    await prisma.depositChannel.create({
      data: {
        srcChain: 'Bitcoin',
        depositAddress:
          'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
        channelId: 3,
        issuedBlock: 0,
        isSwapping: true,
      },
    });

    const swapDepositChannel = await prisma.swapDepositChannel.create({
      data: {
        srcAsset: 'BTC',
        srcChain: 'Bitcoin',
        srcChainExpiryBlock: 100,
        depositAddress:
          'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
        expectedDepositAmount: 0,
        destAddress: '0x6fd76a7699e6269af49e9c63f01f61464ab21d1c',
        brokerCommissionBps: 0,
        destAsset: 'ETH',
        channelId: 3,
        issuedBlock: 0,
        swaps: {
          create: {
            swapInputAmount: '100000',
            depositAmount: '100000',
            srcAsset: 'BTC',
            destAsset: 'ETH',
            destAddress: '0x6fd76a7699e6269af49e9c63f01f61464ab21d1c',
            type: 'SWAP',
            nativeId: 1,
            depositReceivedAt: new Date(1670337099000),
            depositReceivedBlockIndex: `0-15`,
          } as any,
        },
      },
    });

    await prisma.$transaction(async (txClient) => {
      await networkDepositReceived('Bitcoin')({
        prisma: txClient,
        block: networkDepositReceivedBtcMock.block as any,
        event: networkDepositReceivedBtcMock.eventContext.event as any,
      });
    });

    const swap = await prisma.swap.findFirstOrThrow({
      where: { swapDepositChannelId: swapDepositChannel.id },
      include: { fees: true },
    });

    expect(swap.depositAmount.toString()).toBe('110000');
    expect(swap).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
      updatedAt: expect.any(Date),
      swapDepositChannelId: expect.any(BigInt),
      fees: [{ id: expect.any(BigInt), swapId: expect.any(BigInt) }],
    });
  });

  it('should ignore deposit if there is no swap for deposit channel', async () => {
    await prisma.depositChannel.create({
      data: {
        srcChain: 'Bitcoin',
        depositAddress:
          'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
        channelId: 3,
        issuedBlock: 0,
        isSwapping: true,
      },
    });

    await prisma.swapDepositChannel.create({
      data: {
        srcAsset: 'BTC',
        srcChain: 'Bitcoin',
        srcChainExpiryBlock: 100,
        depositAddress:
          'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
        expectedDepositAmount: 0,
        destAddress: '0x6fd76a7699e6269af49e9c63f01f61464ab21d1c',
        brokerCommissionBps: 0,
        destAsset: 'ETH',
        channelId: 3,
        issuedBlock: 0,
      },
    });

    await prisma.$transaction(async (txClient) => {
      await networkDepositReceived('Bitcoin')({
        prisma: txClient,
        block: networkDepositReceivedBtcMock.block as any,
        event: networkDepositReceivedBtcMock.eventContext.event as any,
      });
    });

    const swaps = await prisma.swap.findMany();
    expect(swaps).toHaveLength(0);
  });

  it('should not change swap if there is a newer deposit channel', async () => {
    // swap deposit channel
    await prisma.depositChannel.create({
      data: {
        srcChain: 'Bitcoin',
        depositAddress:
          'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
        channelId: 3,
        issuedBlock: 0,
        isSwapping: true,
      },
    });

    const swapDepositChannel = await prisma.swapDepositChannel.create({
      data: {
        srcAsset: 'BTC',
        srcChain: 'Bitcoin',
        srcChainExpiryBlock: 100,
        depositAddress:
          'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
        expectedDepositAmount: 0,
        destAddress: '0x6fd76a7699e6269af49e9c63f01f61464ab21d1c',
        brokerCommissionBps: 0,
        destAsset: 'ETH',
        channelId: 3,
        issuedBlock: 0,
        swaps: {
          create: {
            swapInputAmount: '100000',
            depositAmount: '100000',
            srcAsset: 'BTC',
            destAsset: 'ETH',
            destAddress: '0x6fd76a7699e6269af49e9c63f01f61464ab21d1c',
            type: 'SWAP',
            nativeId: 1,
            depositReceivedAt: new Date(1670337099000),
            depositReceivedBlockIndex: `0-15`,
          } as any,
        },
      },
    });

    // liqudity deposit channel
    await prisma.depositChannel.create({
      data: {
        srcChain: 'Bitcoin',
        depositAddress:
          'tb1pdz3akc5wa2gr69v3x87tfg0ka597dxqvfl6zhqx4y202y63cgw0q3rgpm6',
        channelId: 3,
        issuedBlock: 2,
        isSwapping: false,
      },
    });

    await prisma.$transaction(async (txClient) => {
      await networkDepositReceived('Bitcoin')({
        prisma: txClient,
        block: networkDepositReceivedBtcMock.block as any,
        event: networkDepositReceivedBtcMock.eventContext.event as any,
      });
    });

    const swap = await prisma.swap.findFirstOrThrow({
      where: { swapDepositChannelId: swapDepositChannel.id },
      include: { fees: true },
    });

    // check that swap was not changed
    expect(swap.depositAmount.toString()).toBe('100000');
    expect(swap).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
      updatedAt: expect.any(Date),
      swapDepositChannelId: expect.any(BigInt),
    });
  });
});

describe('depositReceivedArgs', () => {
  it.each([
    {
      asset: { __kind: 'Btc' },
      amount: '1000000',
      depositAddress: {
        value:
          '0x69e988bde97e4b988f1d11fa362118c4bdf5e32c45a6e7e89fde3106b764bebd',
        __kind: 'Taproot',
      },
      depositDetails: {
        txId: '0x14fd88f956c399e64356546fea41ba234670a7b63c8e2b7e81c8f1ae9011b0d7',
        vout: 1,
      },
    },
    {
      asset: { __kind: 'Flip' },
      amount: '9853636405123772134',
      depositAddress: '0xe0c0ca3540ddd2fc6244e62aa8c8f70c7021ff7f',
    },
    {
      asset: { __kind: 'Usdc' },
      amount: '1000000000',
      depositAddress: '0x9a53bd378c459f71a74acd96bfcd64ed96d92a8e',
    },
    {
      asset: { __kind: 'Eth' },
      amount: '100000000000000000',
      depositAddress: '0xf7b277413fd3e1f1d1e631b1b121443889e46719',
    },
    {
      asset: { __kind: 'Dot' },
      amount: '30000000000',
      depositAddress:
        '0x2d369b6db7f9dc6f332ca5887208d5814dcd759a516ee2507f9582d8b25d7b97',
    },
  ])('parses the event', (args) => {
    expect(depositReceivedArgs.safeParse(args).success).toBe(true);
  });
});
