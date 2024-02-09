import { networkEgressScheduledMock } from './utils';
import prisma from '../../client';
import networkEgressScheduled from '../networkEgressScheduled';

describe(networkEgressScheduled, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "Egress" CASCADE`;
  });

  it('creates an egress entity with the correct data', async () => {
    const { block } = networkEgressScheduledMock;
    const { event } = networkEgressScheduledMock.eventContext;

    await prisma.$transaction((tx) =>
      networkEgressScheduled({
        block: block as any,
        event: event as any,
        prisma: tx,
      }),
    );

    const egress = await prisma.egress.findFirstOrThrow({
      where: { scheduledBlockIndex: `${block.height}-${event.indexInBlock}` },
    });

    expect(egress).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
      updatedAt: expect.any(Date),
    });
  });
});
