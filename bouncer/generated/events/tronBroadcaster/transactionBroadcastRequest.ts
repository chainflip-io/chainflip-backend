import { z } from 'zod';
import {
  accountId,
  cfChainsEvmSchnorrVerificationComponents,
  cfChainsTronTronTransaction,
} from '../common';

export const tronBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsTronTronTransaction,
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
});
