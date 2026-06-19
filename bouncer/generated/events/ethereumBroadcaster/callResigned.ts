import { z } from 'zod';
import { cfChainsEvmTransaction } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsEvmTransaction,
});

export const ethereumBroadcasterCallResignedEvent = defineEvent(
  'EthereumBroadcaster.CallResigned',
  ethereumBroadcasterCallResigned,
);
