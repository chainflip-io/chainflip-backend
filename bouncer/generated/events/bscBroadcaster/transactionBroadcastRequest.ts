import { z } from 'zod';
import {
  accountId,
  cfChainsEvmSchnorrVerificationComponents,
  cfChainsEvmTransaction,
} from '../common';

export const bscBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsEvmTransaction,
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
});
