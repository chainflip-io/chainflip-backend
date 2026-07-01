import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const bscIngressEgressUtxoConsolidationEvent = defineEvent(
  'BscIngressEgress.UtxoConsolidation',
  bscIngressEgressUtxoConsolidation,
);
