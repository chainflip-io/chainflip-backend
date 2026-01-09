import { z } from 'zod';

export const arbitrumIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
