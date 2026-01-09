import { z } from 'zod';

export const bitcoinIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
