import { z } from 'zod';
import { cfChainsEvmTransaction } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsEvmTransaction,
});

export const bscBroadcasterCallResignedEvent = defineEvent(
  'BscBroadcaster.CallResigned',
  bscBroadcasterCallResigned,
);
