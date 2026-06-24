import { z } from 'zod';
import { cfChainsEvmDepositDetails, palletCfBscIngressEgressRefundFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfBscIngressEgressRefundFailureReason,
});

export const bscIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'BscIngressEgress.TransactionRejectionFailed',
  bscIngressEgressTransactionRejectionFailed,
);
