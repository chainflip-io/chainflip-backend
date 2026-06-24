import { z } from 'zod';
import { cfChainsDotPolkadotTransactionData } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsDotPolkadotTransactionData,
});

export const polkadotBroadcasterCallResignedEvent = defineEvent(
  'PolkadotBroadcaster.CallResigned',
  polkadotBroadcasterCallResigned,
);
