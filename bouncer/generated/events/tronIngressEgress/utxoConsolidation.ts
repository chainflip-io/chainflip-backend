import { z } from 'zod';

export const tronIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
