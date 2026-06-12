import { z } from 'zod';
import {
  cfChainsEvmDepositDetails,
  palletCfEthereumIngressEgressRefundFailureReason,
} from '../common';

export const ethereumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
  reason: palletCfEthereumIngressEgressRefundFailureReason,
});
