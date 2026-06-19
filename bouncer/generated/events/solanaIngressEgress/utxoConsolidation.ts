import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const solanaIngressEgressUtxoConsolidationEvent = defineEvent(
  'SolanaIngressEgress.UtxoConsolidation',
  solanaIngressEgressUtxoConsolidation,
);
