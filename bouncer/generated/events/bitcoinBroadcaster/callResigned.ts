import { z } from 'zod';
import { cfChainsBtcBitcoinTransactionData } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsBtcBitcoinTransactionData,
});

export const bitcoinBroadcasterCallResignedEvent = defineEvent(
  'BitcoinBroadcaster.CallResigned',
  bitcoinBroadcasterCallResigned,
);
