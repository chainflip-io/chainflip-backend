import { z } from 'zod';
import { cfChainsSolSolanaTransactionData } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsSolSolanaTransactionData,
});

export const solanaBroadcasterCallResignedEvent = defineEvent(
  'SolanaBroadcaster.CallResigned',
  solanaBroadcasterCallResigned,
);
