import { z } from 'zod';

export const assethubIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
