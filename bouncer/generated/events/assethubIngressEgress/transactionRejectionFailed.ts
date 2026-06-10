import { z } from 'zod';
import { palletCfAssethubIngressEgressRefundFailureReason } from '../common';

export const assethubIngressEgressTransactionRejectionFailed = z.object({
  txId: z.number(),
  reason: palletCfAssethubIngressEgressRefundFailureReason,
});
