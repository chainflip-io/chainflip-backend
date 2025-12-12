import { z } from 'zod';

export const polkadotIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });
