import { z } from 'zod';
import { cfChainsBtcUtxo, palletCfBitcoinIngressEgressRefundFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsBtcUtxo,
  reason: palletCfBitcoinIngressEgressRefundFailureReason,
});

export const bitcoinIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'BitcoinIngressEgress.TransactionRejectionFailed',
  bitcoinIngressEgressTransactionRejectionFailed,
);
