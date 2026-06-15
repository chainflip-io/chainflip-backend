import { z } from 'zod';
import { palletCfAssethubIngressEgressRefundFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransactionRejectionFailed = z.object({
  txId: z.number(),
  reason: palletCfAssethubIngressEgressRefundFailureReason,
});

export const assethubIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'AssethubIngressEgress.TransactionRejectionFailed',
  assethubIngressEgressTransactionRejectionFailed,
);
