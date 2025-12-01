import { z } from 'zod';
import {
  accountId,
  cfChainsEvmSchnorrVerificationComponents,
  cfChainsEvmTransaction,
} from '../common';

export const ethereumBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsEvmTransaction,
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
});
