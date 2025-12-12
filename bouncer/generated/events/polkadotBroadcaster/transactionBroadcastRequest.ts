import { z } from 'zod';
import { accountId, cfChainsDotPolkadotTransactionData, hexString } from '../common';

export const polkadotBroadcasterTransactionBroadcastRequest = z.object({
  broadcastId: z.number(),
  nominee: accountId,
  transactionPayload: cfChainsDotPolkadotTransactionData,
  transactionOutId: hexString,
});
