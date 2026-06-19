import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressTransactionRejectionFailed = z.object({
  txId: cfChainsEvmDepositDetails,
});

export const arbitrumIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'ArbitrumIngressEgress.TransactionRejectionFailed',
  arbitrumIngressEgressTransactionRejectionFailed,
);
