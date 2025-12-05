import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const arbitrumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
});
