import { Chains } from '@/shared/enums';
import {
  createChainTrackingInfo,
  createDepositChannel,
  swapDepositAddressReadyCcmMetadataMocked,
  swapDepositAddressReadyMocked,
} from './utils';
import prisma from '../../client';
import swapDepositAddressReady from '../swapDepositAddressReady';

const eventMock = swapDepositAddressReadyMocked;
const ccmEventMock = swapDepositAddressReadyCcmMetadataMocked;

describe(swapDepositAddressReady, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel" CASCADE`;
    await prisma.$queryRaw`TRUNCATE TABLE "ChainTracking" CASCADE`;
  });

  it('creates a swap deposit channel entry', async () => {
    await prisma.$transaction(async (txClient) => {
      await createChainTrackingInfo();
      await swapDepositAddressReady({
        prisma: txClient,
        event: eventMock.eventContext.event,
        block: eventMock.block,
      });
    });

    const swapDepositChannel = await prisma.swapDepositChannel.findFirstOrThrow(
      {
        where: {
          channelId: BigInt(eventMock.eventContext.event.args.channelId),
        },
      },
    );

    expect(swapDepositChannel).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
    });
  });

  it('creates a swap deposit channel entry with a broker commission', async () => {
    await prisma.$transaction(async (txClient) => {
      await createChainTrackingInfo();
      await swapDepositAddressReady({
        prisma: txClient,
        event: {
          ...eventMock.eventContext.event,
          args: {
            ...eventMock.eventContext.event.args,
            brokerCommissionRate: 25,
          },
        },
        block: { ...eventMock.block, height: 121 },
      });
    });

    const swapDepositChannel = await prisma.swapDepositChannel.findFirstOrThrow(
      {
        where: {
          channelId: BigInt(eventMock.eventContext.event.args.channelId),
        },
      },
    );

    expect(swapDepositChannel).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
    });
  });

  it('creates a swap deposit channel entry with ccm metadata', async () => {
    await prisma.$transaction(async (txClient) => {
      await createChainTrackingInfo();
      await swapDepositAddressReady({
        prisma: txClient,
        event: ccmEventMock.eventContext.event,
        block: ccmEventMock.block,
      });
    });

    const swapDepositChannel = await prisma.swapDepositChannel.findFirstOrThrow(
      {
        where: {
          channelId: BigInt(ccmEventMock.eventContext.event.args.channelId),
        },
      },
    );

    expect(swapDepositChannel).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
    });
  });

  it('does not overwrite expectedDepositAmount with zero', async () => {
    await createDepositChannel({
      channelId: BigInt(eventMock.eventContext.event.args.channelId),
      srcChain: Chains.Ethereum,
      issuedBlock: 10,
      expectedDepositAmount: 650,
    });

    await prisma.$transaction(async (txClient) => {
      await swapDepositAddressReady({
        prisma: txClient,
        event: eventMock.eventContext.event,
        block: {
          ...eventMock.block,
          height: 10,
        },
      });
    });

    const swapDepositChannel = await prisma.swapDepositChannel.findFirstOrThrow(
      {
        where: {
          channelId: BigInt(eventMock.eventContext.event.args.channelId),
        },
      },
    );

    expect(swapDepositChannel).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
    });
  });
});
