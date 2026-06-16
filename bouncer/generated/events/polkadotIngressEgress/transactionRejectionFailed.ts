import { z } from 'zod';
import { palletCfPolkadotIngressEgressRefundFailureReason } from '../common';

export const polkadotIngressEgressTransactionRejectionFailed = z.object({
  txId: z.number(),
  reason: palletCfPolkadotIngressEgressRefundFailureReason,
});
