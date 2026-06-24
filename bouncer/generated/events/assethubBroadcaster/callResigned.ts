import { z } from 'zod';
import { cfChainsDotPolkadotTransactionData } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsDotPolkadotTransactionData,
});

export const assethubBroadcasterCallResignedEvent = defineEvent(
  'AssethubBroadcaster.CallResigned',
  assethubBroadcasterCallResigned,
);
