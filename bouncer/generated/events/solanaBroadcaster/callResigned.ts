import { z } from 'zod';
import { cfChainsSolSolanaTransactionData } from '../common';

export const solanaBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsSolSolanaTransactionData,
});
