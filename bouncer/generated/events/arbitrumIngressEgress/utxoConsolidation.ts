import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const arbitrumIngressEgressUtxoConsolidationEvent = defineEvent(
  'ArbitrumIngressEgress.UtxoConsolidation',
  arbitrumIngressEgressUtxoConsolidation,
);
