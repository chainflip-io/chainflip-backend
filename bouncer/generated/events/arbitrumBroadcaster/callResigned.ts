import { z } from 'zod';
import { cfChainsEvmTransaction } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsEvmTransaction,
});

export const arbitrumBroadcasterCallResignedEvent = defineEvent(
  'ArbitrumBroadcaster.CallResigned',
  arbitrumBroadcasterCallResigned,
);
