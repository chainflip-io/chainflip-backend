import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
});

export const ethereumIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'EthereumIngressEgress.TransactionRejectionFailed',
  ethereumIngressEgressTransactionRejectionFailed,
);
