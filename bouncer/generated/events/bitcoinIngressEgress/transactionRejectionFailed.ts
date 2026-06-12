import { z } from 'zod';
import { cfChainsBtcUtxo, palletCfBitcoinIngressEgressRefundFailureReason } from '../common';

export const bitcoinIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsBtcUtxo,
  reason: palletCfBitcoinIngressEgressRefundFailureReason,
});
