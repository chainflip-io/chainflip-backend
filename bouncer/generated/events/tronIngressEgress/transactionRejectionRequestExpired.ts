import { z } from 'zod';
import { accountId, hexString } from '../common';

export const tronIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});
