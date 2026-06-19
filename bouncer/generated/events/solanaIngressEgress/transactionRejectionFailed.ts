import { z } from 'zod';
import { cfChainsSolVaultSwapOrDepositChannelId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsSolVaultSwapOrDepositChannelId,
});

export const solanaIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'SolanaIngressEgress.TransactionRejectionFailed',
  solanaIngressEgressTransactionRejectionFailed,
);
