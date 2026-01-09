import { z } from 'zod';

export const solanaIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
