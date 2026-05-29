import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
});

export const tronIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'TronIngressEgress.TransactionRejectionFailed',
  tronIngressEgressTransactionRejectionFailed,
);
