import { z } from 'zod';
import { cfChainsEvmDepositDetails, palletCfTronIngressEgressRefundFailureReason } from '../common';

export const tronIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfTronIngressEgressRefundFailureReason,
});
