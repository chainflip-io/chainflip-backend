import { z } from 'zod';
import { hexString, u128, u64 } from '@/shared/parsers';
import { encodedAddress } from './common';
import type { EventHandlerArgs } from '.';

const ccmDepositReceivedArgs = z.object({
  ccmId: u64,
  principalSwapId: u64.nullable().optional(),
  gasSwapId: u64.nullable().optional(),
  depositAmount: u128,
  destinationAddress: encodedAddress,
  depositMetadata: z.object({
    channelMetadata: z.object({
      message: hexString,
      gasBudget: u128,
    }),
  }),
});

export type CcmDepositReceivedArgs = z.input<typeof ccmDepositReceivedArgs>;

export default async function ccmDepositReceived({
  prisma,
  event,
  block,
}: EventHandlerArgs) {
  const { principalSwapId, depositMetadata } = ccmDepositReceivedArgs.parse(
    event.args,
  );

  if (principalSwapId) {
    await prisma.swap.update({
      where: {
        nativeId: principalSwapId,
      },
      data: {
        ccmDepositReceivedBlockIndex: `${block.height}-${event.indexInBlock}`,
        ccmGasBudget: depositMetadata.channelMetadata.gasBudget.toString(),
        ccmMessage: depositMetadata.channelMetadata.message,
      },
    });
  }
}
