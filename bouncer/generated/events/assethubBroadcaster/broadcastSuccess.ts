import { z } from 'zod';
import { cfChainsDotPolkadotTransactionId, hexString } from '../common';

export const assethubBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: hexString,
  transactionRef: cfChainsDotPolkadotTransactionId,
});
