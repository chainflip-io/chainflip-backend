import { Assets } from '@/shared/enums';
import prisma from '@/swap/client';
import { DOT_ADDRESS, createDepositChannel } from './utils';
import ccmDepositReceived from '../ccmDepositReceived';

describe(ccmDepositReceived, () => {
  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel", "Swap" CASCADE`;
  });

  it('happy case', async () => {
    const block = {
      timestamp: Date.now(),
      height: 1000,
    };
    const indexInBlock = 5;
    await createDepositChannel({
      swaps: {
        create: {
          nativeId: BigInt(9876545),
          depositAmount: '10000000000',
          swapInputAmount: '10000000000',
          depositReceivedAt: new Date(Date.now() - 6000),
          depositReceivedBlockIndex: `${block.height}-${indexInBlock}`,
          srcAsset: Assets.ETH,
          destAsset: Assets.DOT,
          destAddress: DOT_ADDRESS,
          type: 'SWAP',
        },
      },
    });

    await prisma.$transaction(async (client) => {
      await ccmDepositReceived({
        prisma: client,
        block: block as any,
        event: {
          args: {
            ccmId: '150',
            depositAmount: '3829832913',
            destinationAddress: {
              value: '0x41ad2bc63a2059f9b623533d87fe99887d794847',
              __kind: 'Eth',
            },
            principalSwapId: '9876545',
            depositMetadata: {
              channelMetadata: {
                gasBudget: '65000',
                message: '0x12abf87',
              },
            },
          },
          name: 'ccmDepositReceived',
          indexInBlock: 6,
        },
      });
    });

    const swap = await prisma.swap.findFirstOrThrow({
      where: { nativeId: BigInt(9876545) },
    });

    expect(swap.ccmGasBudget?.toString()).toEqual('65000');
    expect(swap.ccmMessage).toEqual('0x12abf87');
    expect(swap.ccmDepositReceivedBlockIndex).toEqual('1000-6');
  });
});
