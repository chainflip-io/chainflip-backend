import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';

export const polkadotIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: cfPrimitivesTxId,
  expiresAt: z.number(),
});
