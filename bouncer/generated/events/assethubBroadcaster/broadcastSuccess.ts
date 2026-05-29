import { z } from 'zod';
import { cfChainsDotPolkadotTransactionId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: hexString,
  transactionRef: cfChainsDotPolkadotTransactionId,
});

export const assethubBroadcasterBroadcastSuccessEvent = defineEvent(
  'AssethubBroadcaster.BroadcastSuccess',
  assethubBroadcasterBroadcastSuccess,
);
