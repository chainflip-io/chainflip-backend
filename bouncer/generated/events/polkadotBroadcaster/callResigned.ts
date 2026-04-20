import { z } from 'zod';
import { cfChainsDotPolkadotTransactionData } from '../common';

export const polkadotBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsDotPolkadotTransactionData,
});
