import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const tronIngressEgressUtxoConsolidationEvent = defineEvent(
  'TronIngressEgress.UtxoConsolidation',
  tronIngressEgressUtxoConsolidation,
);
