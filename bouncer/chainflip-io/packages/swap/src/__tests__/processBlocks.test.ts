import { GraphQLClient } from 'graphql-request';
import prisma from '../client';
import { swapScheduledDotDepositChannelMock } from '../event-handlers/__tests__/utils';
import { GetBatchQuery } from '../gql/generated/graphql';
import processBlocks from '../processBlocks';

describe(processBlocks, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel", "Swap", private."State" CASCADE`;
  });

  it('dispatches a SwapScheduled event', async () => {
    await prisma.swapDepositChannel.create({
      data: {
        depositAddress: '5CGLqaFMheyVcsXz6QEtjtSAi6RcXFaEDJKvovgCdPiZhw11',
        issuedBlock: 100,
        expiryBlock: 200,
        channelId: 250n,
        srcChain: 'Polkadot',
        srcAsset: 'DOT',
        destAsset: 'BTC',
        destAddress: 'bcrt1pzjdpc799qa5f7m65hpr66880res5ac3lr6y2chc4jsa',
        expectedDepositAmount: '1000000000000000000',
      },
    });

    await prisma.state.upsert({
      where: { id: 1 },
      create: { id: 1, height: 149 },
      update: { height: 1 },
    });

    const requestSpy = jest
      .spyOn(GraphQLClient.prototype, 'request')
      .mockResolvedValueOnce({
        blocks: {
          nodes: [
            {
              height: 150,
              timestamp: 1681989543437,
              specId: 'chainflip-node@0',
              events: {
                nodes: [swapScheduledDotDepositChannelMock.eventContext.event],
              },
            },
          ],
        },
      } as GetBatchQuery)
      // terminate the loop
      .mockRejectedValue(Error('clean exit'));

    await expect(processBlocks()).rejects.toThrowError('clean exit');
    expect(requestSpy).toHaveBeenCalledTimes(
      1 + // once successfully for the first block
        5, // five failures while we abort the loop
    );
    const swaps = await prisma.swap.findMany();
    expect(swaps).toHaveLength(1);
    expect(swaps[0]).toMatchInlineSnapshot(
      {
        id: expect.any(BigInt),
        swapDepositChannelId: expect.any(BigInt),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date),
      },
      `
      {
        "createdAt": Any<Date>,
        "depositAmount": "125000000000",
        "depositReceivedAt": 2023-04-20T11:19:03.437Z,
        "depositReceivedBlockIndex": "150-0",
        "destAddress": "bcrt1pzjdpc799qa5f7m65hpr66880res5ac3lr6y2chc4jsa",
        "destAsset": "BTC",
        "egressAmount": null,
        "id": Any<BigInt>,
        "intermediateAmount": null,
        "nativeId": 1n,
        "srcAsset": "DOT",
        "swapDepositChannelId": Any<BigInt>,
        "swapExecutedAt": null,
        "swapExecutedBlockIndex": null,
        "txHash": null,
        "updatedAt": Any<Date>,
      }
    `,
    );
  });
});
