import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const ethereumIngressEgressUtxoConsolidationEvent = defineEvent(
  'EthereumIngressEgress.UtxoConsolidation',
  ethereumIngressEgressUtxoConsolidation,
);
