import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';

export const assethubIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: cfPrimitivesTxId,
});
