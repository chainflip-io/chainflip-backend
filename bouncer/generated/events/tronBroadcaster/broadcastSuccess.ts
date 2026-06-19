import { z } from 'zod';
import { cfChainsEvmSchnorrVerificationComponents, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
  transactionRef: hexString,
});

export const tronBroadcasterBroadcastSuccessEvent = defineEvent(
  'TronBroadcaster.BroadcastSuccess',
  tronBroadcasterBroadcastSuccess,
);
