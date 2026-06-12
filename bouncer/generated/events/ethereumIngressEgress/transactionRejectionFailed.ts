import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const ethereumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
});
