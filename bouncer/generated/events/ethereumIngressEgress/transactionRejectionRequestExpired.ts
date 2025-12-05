import { z } from 'zod';
import { accountId, hexString } from '../common';

export const ethereumIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});
