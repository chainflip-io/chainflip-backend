import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const bscIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
});
