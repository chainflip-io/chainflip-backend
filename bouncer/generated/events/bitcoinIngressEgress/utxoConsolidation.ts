import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const bitcoinIngressEgressUtxoConsolidationEvent = defineEvent(
  'BitcoinIngressEgress.UtxoConsolidation',
  bitcoinIngressEgressUtxoConsolidation,
);
