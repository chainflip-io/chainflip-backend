import { z } from 'zod';
import {
  accountId,
  cfChainsEvmSchnorrVerificationComponents,
  cfChainsEvmTransaction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsEvmTransaction,
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
});

export const arbitrumBroadcasterTransactionBroadcastRequestEvent = defineEvent(
  'ArbitrumBroadcaster.TransactionBroadcastRequest',
  arbitrumBroadcasterTransactionBroadcastRequest,
);
