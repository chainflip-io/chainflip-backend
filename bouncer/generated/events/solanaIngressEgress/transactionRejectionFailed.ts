import { z } from 'zod';
import {
  cfChainsSolVaultSwapOrDepositChannelId,
  palletCfSolanaIngressEgressRefundFailureReason,
} from '../common';

export const solanaIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsSolVaultSwapOrDepositChannelId,
  reason: palletCfSolanaIngressEgressRefundFailureReason,
});
