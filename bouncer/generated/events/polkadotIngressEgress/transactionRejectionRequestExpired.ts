import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';

export const polkadotIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: cfPrimitivesTxId,
});
