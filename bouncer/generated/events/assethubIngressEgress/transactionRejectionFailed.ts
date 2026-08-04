import { z } from 'zod';
import { cfPrimitivesTxId, palletCfAssethubIngressEgressRefundFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransactionRejectionFailed = z.object({
  txId: cfPrimitivesTxId,
  reason: palletCfAssethubIngressEgressRefundFailureReason,
});

export const assethubIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'AssethubIngressEgress.TransactionRejectionFailed',
  assethubIngressEgressTransactionRejectionFailed,
);
