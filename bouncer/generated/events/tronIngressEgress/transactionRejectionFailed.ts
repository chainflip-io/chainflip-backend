import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';

export const tronIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
});
