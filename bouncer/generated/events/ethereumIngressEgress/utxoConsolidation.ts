import { z } from 'zod';

export const ethereumIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
