import { Assets } from '@/shared/enums';
import prisma, { SwapDepositChannel } from '../../client';
import swapScheduled from '../swapScheduled';
import {
  createDepositChannel,
  swapScheduledBtcDepositChannelMock,
  swapScheduledDotDepositChannelMock,
  swapScheduledVaultMock,
} from './utils';

describe(swapScheduled, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel", "Swap" CASCADE`;
  });

  describe('deposit channel origin', () => {
    let dotSwapDepositChannel: SwapDepositChannel;
    let btcSwapDepositChannel: SwapDepositChannel;

    beforeEach(async () => {
      dotSwapDepositChannel = await createDepositChannel({
        srcChain: 'Polkadot',
        srcAsset: Assets.DOT,
        destAsset: Assets.BTC,
        depositAddress: '5CGLqaFMheyVcsXz6QEtjtSAi6RcXFaEDJKvovgCdPiZhw11',
        destAddress: 'bcrt1pzjdpc799qa5f7m65hpr66880res5ac3lr6y2chc4jsa',
      });
      btcSwapDepositChannel = await createDepositChannel({
        srcChain: 'Bitcoin',
        srcAsset: Assets.BTC,
        destAsset: Assets.ETH,
        depositAddress: 'bcrt1pzjdpc799qa5f7m65hpr66880res5ac3lr6y2chc4jsa',
        destAddress: '0x41ad2bc63a2059f9b623533d87fe99887d794847',
      });
    });

    it('stores a new swap from a dot deposit channel', async () => {
      await prisma.$transaction(async (client) => {
        await swapScheduled({
          prisma: client,
          block: swapScheduledDotDepositChannelMock.block as any,
          event: swapScheduledDotDepositChannelMock.eventContext.event as any,
        });
      });

      const swap = await prisma.swap.findFirstOrThrow({
        where: { swapDepositChannelId: dotSwapDepositChannel.id },
      });

      expect(swap.depositAmount.toString()).toEqual(
        swapScheduledDotDepositChannelMock.eventContext.event.args
          .depositAmount,
      );
      expect(swap).toMatchSnapshot({
        id: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
        swapDepositChannelId: expect.any(BigInt),
      });
    });

    it('stores a new swap from a btc deposit channel', async () => {
      await prisma.$transaction(async (client) => {
        await swapScheduled({
          prisma: client,
          block: swapScheduledBtcDepositChannelMock.block as any,
          event: swapScheduledBtcDepositChannelMock.eventContext.event as any,
        });
      });

      const swap = await prisma.swap.findFirstOrThrow({
        where: { swapDepositChannelId: btcSwapDepositChannel.id },
      });

      expect(swap.depositAmount.toString()).toEqual(
        swapScheduledBtcDepositChannelMock.eventContext.event.args
          .depositAmount,
      );
      expect(swap).toMatchSnapshot({
        id: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
        swapDepositChannelId: expect.any(BigInt),
      });
    });

    it('does not store a new swap if the deposit channel is expired', async () => {
      await prisma.swapDepositChannel.update({
        where: { id: dotSwapDepositChannel.id },
        data: { expiryBlock: -1 },
      });

      await prisma.$transaction(async (client) => {
        await swapScheduled({
          prisma: client,
          block: swapScheduledDotDepositChannelMock.block,
          event: swapScheduledDotDepositChannelMock.eventContext.event as any,
        });
      });

      expect(await prisma.swap.findFirst()).toBeNull();
    });

    it('does not store a new swap if the deposit channel is not found', async () => {
      await prisma.swapDepositChannel.update({
        where: { id: dotSwapDepositChannel.id },
        data: { depositAddress: '0x0' },
      });

      await prisma.$transaction(async (client) => {
        await swapScheduled({
          prisma: client,
          block: swapScheduledDotDepositChannelMock.block,
          event: swapScheduledDotDepositChannelMock.eventContext.event as any,
        });
      });

      expect(await prisma.swap.findFirst()).toBeNull();
    });

    it('does not store a new swap if the deposit channel is not unique', async () => {
      const { id, ...rest } = dotSwapDepositChannel;
      await prisma.swapDepositChannel.create({
        data: { ...rest, channelId: 2n },
      });

      await expect(
        prisma.$transaction(async (client) => {
          await swapScheduled({
            prisma: client,
            block: swapScheduledDotDepositChannelMock.block,
            event: swapScheduledDotDepositChannelMock.eventContext.event as any,
          });
        }),
      ).rejects.toThrowError();

      expect(await prisma.swap.findFirst()).toBeNull();
    });
  });

  describe('smart contract origin', () => {
    it('stores a new swap from a contract deposit', async () => {
      // create a swap after receiving the event
      await prisma.$transaction(async (client) => {
        await swapScheduled({
          prisma: client,
          block: swapScheduledVaultMock.block as any,
          event: swapScheduledVaultMock.eventContext.event as any,
        });
      });

      const swap = await prisma.swap.findFirstOrThrow();

      expect(swap.depositAmount.toString()).toEqual(
        swapScheduledVaultMock.eventContext.event.args.depositAmount,
      );
      expect(swap).toMatchSnapshot({
        id: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
      });
    });
  });
});
