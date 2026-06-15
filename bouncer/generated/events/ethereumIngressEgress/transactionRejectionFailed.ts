import { z } from 'zod';
import {
  cfChainsEvmDepositDetails,
  palletCfEthereumIngressEgressRefundFailureReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfEthereumIngressEgressRefundFailureReason,
});

export const ethereumIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'EthereumIngressEgress.TransactionRejectionFailed',
  ethereumIngressEgressTransactionRejectionFailed,
);
