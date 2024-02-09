import { thresholdSignatureInvalidMock } from './utils';
import prisma from '../../client';
import networkThresholdSignatureInvalid from '../networkThresholdSignatureInvalid';

describe(networkThresholdSignatureInvalid, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "Egress", "Broadcast" CASCADE`;
  });

  it('handles the replacement of an invalid broadcast', async () => {
    const {
      block,
      eventContext: { event },
    } = thresholdSignatureInvalidMock;

    await prisma.broadcast.create({
      data: {
        nativeId: event.args.broadcastId,
        chain: 'Ethereum',
        requestedAt: new Date(block.timestamp - 12000),
        requestedBlockIndex: `${block.height - 2}-1`,
        egresses: {
          createMany: {
            data: [
              {
                chain: 'Ethereum',
                nativeId: event.args.broadcastId,
                amount: '123456789',
                scheduledAt: new Date(block.timestamp - 6000),
                scheduledBlockIndex: `${block.height - 1}-1`,
              },
              {
                chain: 'Ethereum',
                nativeId: 2n,
                amount: '987654321',
                scheduledAt: new Date(block.timestamp - 6000),
                scheduledBlockIndex: `${block.height - 1}-2`,
              },
            ],
          },
        },
      },
    });

    await prisma.$transaction((tx) =>
      networkThresholdSignatureInvalid('Ethereum')({
        block: block as any,
        event: event as any,
        prisma: tx,
      }),
    );

    const ogBroadcast = await prisma.broadcast.findUniqueOrThrow({
      where: {
        nativeId_chain: { nativeId: event.args.broadcastId, chain: 'Ethereum' },
      },
      include: {
        egresses: { select: { nativeId: true, chain: true, amount: true } },
      },
    });

    expect(ogBroadcast).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
      updatedAt: expect.any(Date),
      replacedById: expect.any(BigInt),
    });

    const retryBroadcast = await prisma.broadcast.findUniqueOrThrow({
      where: {
        nativeId_chain: {
          nativeId: event.args.retryBroadcastId,
          chain: 'Ethereum',
        },
      },
      include: {
        egresses: { select: { nativeId: true, chain: true, amount: true } },
      },
    });

    expect(retryBroadcast).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
      updatedAt: expect.any(Date),
    });
  });
});
