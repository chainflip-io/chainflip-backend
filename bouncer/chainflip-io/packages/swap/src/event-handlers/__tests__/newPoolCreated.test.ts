import { newPoolCreatedMock } from './utils';
import prisma from '../../client';
import newPoolCreated from '../newPoolCreated';

describe(newPoolCreated, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "Pool" CASCADE`;
  });

  it('creates a pool with the correct data', async () => {
    const { block } = newPoolCreatedMock;
    const { event } = newPoolCreatedMock.eventContext;

    await prisma.$transaction((tx) =>
      newPoolCreated({
        block: block as any,
        event: event as any,
        prisma: tx,
      }),
    );

    const pool = await prisma.pool.findFirstOrThrow();

    expect(pool).toMatchSnapshot({
      id: expect.any(Number),
    });
  });
});
