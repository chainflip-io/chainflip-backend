import {
  DOT_ADDRESS,
  swapAmountTooLowBtcDepositChannelMock,
  swapAmountTooLowDotDepositChannelMock,
  swapAmountTooLowVaultMock,
} from './utils';
import prisma from '../../client';
import swapAmountTooLow from '../swapAmountTooLow';

describe(swapAmountTooLow, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel", "Swap" CASCADE`;
  });

  afterEach(async () => {
    jest.resetAllMocks();
  });

  describe('deposit channel origin', () => {
    it('handles a new event from a dot deposit channel', async () => {
      prisma.swapDepositChannel.findFirstOrThrow = jest
        .fn()
        .mockResolvedValueOnce({
          id: 'internal-deposit-channel-id',
        });
      prisma.failedSwap.create = jest.fn();

      await swapAmountTooLow({
        prisma,
        block: swapAmountTooLowDotDepositChannelMock.block as any,
        event: swapAmountTooLowDotDepositChannelMock.eventContext.event as any,
      });

      expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenCalledTimes(
        1,
      );
      expect(
        prisma.swapDepositChannel.findFirstOrThrow,
      ).toHaveBeenNthCalledWith(1, {
        where: {
          srcChain: 'Polkadot',
          depositAddress: DOT_ADDRESS,
          channelId: 2n,
          isExpired: false,
        },
        orderBy: { issuedBlock: 'desc' },
      });
      expect(prisma.failedSwap.create).toHaveBeenCalledTimes(1);
      expect(prisma.failedSwap.create).toHaveBeenNthCalledWith(1, {
        data: {
          destAddress: 'bcrt1pzjdpc799qa5f7m65hpr66880res5ac3lr6y2chc4jsa',
          destChain: 'Bitcoin',
          depositAmount: '12500000000',
          srcChain: 'Polkadot',
          swapDepositChannelId: 'internal-deposit-channel-id',
          txHash: undefined,
          type: 'FAILED',
        },
      });
    });

    it('handles a new event from a btc deposit channel', async () => {
      prisma.swapDepositChannel.findFirstOrThrow = jest
        .fn()
        .mockResolvedValueOnce({
          id: 'internal-deposit-channel-id',
        });
      prisma.failedSwap.create = jest.fn();

      await swapAmountTooLow({
        prisma,
        block: swapAmountTooLowBtcDepositChannelMock.block as any,
        event: swapAmountTooLowBtcDepositChannelMock.eventContext.event as any,
      });

      expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenCalledTimes(
        1,
      );
      expect(
        prisma.swapDepositChannel.findFirstOrThrow,
      ).toHaveBeenNthCalledWith(1, {
        where: {
          srcChain: 'Bitcoin',
          depositAddress: 'bcrt1pzjdpc799qa5f7m65hpr66880res5ac3lr6y2chc4jsa',
          channelId: 2n,
          isExpired: false,
        },
        orderBy: { issuedBlock: 'desc' },
      });
      expect(prisma.failedSwap.create).toHaveBeenCalledTimes(1);
      expect(prisma.failedSwap.create).toHaveBeenNthCalledWith(1, {
        data: {
          destAddress: '0x41ad2bc63a2059f9b623533d87fe99887d794847',
          destChain: 'Ethereum',
          depositAmount: '12500000000',
          srcChain: 'Bitcoin',
          swapDepositChannelId: 'internal-deposit-channel-id',
          txHash: undefined,
          type: 'FAILED',
        },
      });
    });

    it('does not store a swap if the deposit channel is not found', async () => {
      prisma.swapDepositChannel.findFirstOrThrow = jest
        .fn()
        .mockRejectedValueOnce({ message: 'Not found' });
      prisma.failedSwap.create = jest.fn();

      try {
        await swapAmountTooLow({
          prisma,
          block: swapAmountTooLowBtcDepositChannelMock.block as any,
          event: swapAmountTooLowBtcDepositChannelMock.eventContext
            .event as any,
        });
      } catch (err) {
        expect(err).toEqual({ message: 'Not found' });
      }

      expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenCalledTimes(
        1,
      );
      expect(prisma.failedSwap.create).toHaveBeenCalledTimes(0);
    });
  });

  describe('smart contract origin', () => {
    it('handles a new event from a contract deposit', async () => {
      prisma.swapDepositChannel.findFirstOrThrow = jest.fn();
      prisma.failedSwap.create = jest.fn();

      await swapAmountTooLow({
        prisma,
        block: swapAmountTooLowVaultMock.block as any,
        event: swapAmountTooLowVaultMock.eventContext.event as any,
      });

      expect(prisma.swapDepositChannel.findFirstOrThrow).toHaveBeenCalledTimes(
        0,
      );
      expect(prisma.failedSwap.create).toHaveBeenCalledTimes(1);
      expect(prisma.failedSwap.create).toHaveBeenNthCalledWith(1, {
        data: {
          destAddress: DOT_ADDRESS,
          destChain: 'Polkadot',
          depositAmount: '12500000000',
          srcChain: 'Ethereum',
          swapDepositChannelId: undefined,
          txHash:
            '0x1103ebed92b02a278b54789bfabc056e69ad5c6558049364ea23ec2f3bfa0fd9',
          type: 'FAILED',
        },
      });
    });
  });
});
