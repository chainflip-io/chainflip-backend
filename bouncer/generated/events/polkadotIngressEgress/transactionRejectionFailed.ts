import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressTransactionRejectionFailed = z.object({ txId: z.number() });

export const polkadotIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'PolkadotIngressEgress.TransactionRejectionFailed',
  polkadotIngressEgressTransactionRejectionFailed,
);
