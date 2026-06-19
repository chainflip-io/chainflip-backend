import { z } from 'zod';
import { accountId, cfChainsDotPolkadotTransactionData, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsDotPolkadotTransactionData,
  transactionOutId: hexString,
});

export const assethubBroadcasterTransactionBroadcastRequestEvent = defineEvent(
  'AssethubBroadcaster.TransactionBroadcastRequest',
  assethubBroadcasterTransactionBroadcastRequest,
);
