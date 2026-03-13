import { z } from 'zod';

export const bscIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
