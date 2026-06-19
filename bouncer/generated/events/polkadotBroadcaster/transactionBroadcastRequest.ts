import { z } from 'zod';
import { accountId, cfChainsDotPolkadotTransactionData, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsDotPolkadotTransactionData,
  transactionOutId: hexString,
});

export const polkadotBroadcasterTransactionBroadcastRequestEvent = defineEvent(
  'PolkadotBroadcaster.TransactionBroadcastRequest',
  polkadotBroadcasterTransactionBroadcastRequest,
);
