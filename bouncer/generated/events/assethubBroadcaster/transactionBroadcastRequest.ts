import { z } from 'zod';
import { accountId, cfChainsDotPolkadotTransactionData, hexString } from '../common';

export const assethubBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsDotPolkadotTransactionData,
  transactionOutId: hexString,
});
