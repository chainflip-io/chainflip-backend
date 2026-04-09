import { z } from 'zod';
import { cfChainsBtcBitcoinTransactionData } from '../common';

export const bitcoinBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsBtcBitcoinTransactionData,
});
