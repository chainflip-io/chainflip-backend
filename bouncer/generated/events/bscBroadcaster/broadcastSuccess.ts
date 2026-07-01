import { z } from 'zod';
import { cfChainsEvmSchnorrVerificationComponents, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
  transactionRef: hexString,
});

export const bscBroadcasterBroadcastSuccessEvent = defineEvent(
  'BscBroadcaster.BroadcastSuccess',
  bscBroadcasterBroadcastSuccess,
);
