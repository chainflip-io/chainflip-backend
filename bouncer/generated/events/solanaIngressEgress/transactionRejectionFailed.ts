import { z } from 'zod';
import { cfChainsSolVaultSwapOrDepositChannelId } from '../common';

export const solanaIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsSolVaultSwapOrDepositChannelId,
});
