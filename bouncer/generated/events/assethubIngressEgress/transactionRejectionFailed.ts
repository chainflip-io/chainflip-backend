import { z } from 'zod';

export const assethubIngressEgressTransactionRejectionFailed = z.object({ txId: z.number() });
