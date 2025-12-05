import { z } from 'zod';

export const polkadotIngressEgressTransactionRejectionFailed = z.object({ txId: z.number() });
