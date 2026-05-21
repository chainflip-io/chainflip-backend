import { z } from 'zod';
import { cfChainsDotPolkadotTransactionData } from '../common';

export const assethubBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsDotPolkadotTransactionData,
});
