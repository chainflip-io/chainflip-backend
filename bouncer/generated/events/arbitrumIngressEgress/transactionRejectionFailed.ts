import { z } from 'zod';
import {
  cfChainsEvmDepositDetails,
  palletCfArbitrumIngressEgressRefundFailureReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfArbitrumIngressEgressRefundFailureReason,
});

export const arbitrumIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'ArbitrumIngressEgress.TransactionRejectionFailed',
  arbitrumIngressEgressTransactionRejectionFailed,
);
