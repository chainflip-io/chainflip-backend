import { z } from 'zod';
import {
  accountId,
  cfChainsEvmSchnorrVerificationComponents,
  cfChainsTronTronTransaction,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsTronTronTransaction,
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
});

export const tronBroadcasterTransactionBroadcastRequestEvent = defineEvent(
  'TronBroadcaster.TransactionBroadcastRequest',
  tronBroadcasterTransactionBroadcastRequest,
);
