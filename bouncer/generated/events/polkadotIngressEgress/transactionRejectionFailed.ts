import { z } from 'zod';
import { palletCfPolkadotIngressEgressRefundFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressTransactionRejectionFailed = z.object({
  txId: z.number(),
  reason: palletCfPolkadotIngressEgressRefundFailureReason,
});

export const polkadotIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'PolkadotIngressEgress.TransactionRejectionFailed',
  polkadotIngressEgressTransactionRejectionFailed,
);
