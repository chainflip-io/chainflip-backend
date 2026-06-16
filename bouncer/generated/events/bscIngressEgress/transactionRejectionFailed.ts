import { z } from 'zod';
import { cfChainsEvmDepositDetails, palletCfBscIngressEgressRefundFailureReason } from '../common';

export const bscIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfBscIngressEgressRefundFailureReason,
});
