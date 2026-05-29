import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const assethubIngressEgressUtxoConsolidationEvent = defineEvent(
  'AssethubIngressEgress.UtxoConsolidation',
  assethubIngressEgressUtxoConsolidation,
);
