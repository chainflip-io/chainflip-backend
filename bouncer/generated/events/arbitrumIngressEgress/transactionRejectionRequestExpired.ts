import { z } from 'zod';
import { accountId, hexString } from '../common';

export const arbitrumIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});
