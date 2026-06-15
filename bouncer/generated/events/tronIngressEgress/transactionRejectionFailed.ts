import { z } from 'zod';
import { cfChainsEvmDepositDetails, palletCfTronIngressEgressRefundFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfTronIngressEgressRefundFailureReason,
});

export const tronIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'TronIngressEgress.TransactionRejectionFailed',
  tronIngressEgressTransactionRejectionFailed,
);
