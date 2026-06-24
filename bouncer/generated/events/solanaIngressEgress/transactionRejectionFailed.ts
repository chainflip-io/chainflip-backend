import { z } from 'zod';
import {
  cfChainsSolVaultSwapOrDepositChannelId,
  palletCfSolanaIngressEgressRefundFailureReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsSolVaultSwapOrDepositChannelId,
  reason: palletCfSolanaIngressEgressRefundFailureReason,
});

export const solanaIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'SolanaIngressEgress.TransactionRejectionFailed',
  solanaIngressEgressTransactionRejectionFailed,
);
