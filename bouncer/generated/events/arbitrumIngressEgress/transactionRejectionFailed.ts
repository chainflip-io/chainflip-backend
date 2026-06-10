import { z } from 'zod';
import {
  cfChainsEvmDepositDetails,
  palletCfArbitrumIngressEgressRefundFailureReason,
} from '../common';

export const arbitrumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfArbitrumIngressEgressRefundFailureReason,
});
