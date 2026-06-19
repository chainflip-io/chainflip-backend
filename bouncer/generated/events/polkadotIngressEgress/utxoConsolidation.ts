import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressUtxoConsolidation = z.object({ broadcastId: z.number() });

export const polkadotIngressEgressUtxoConsolidationEvent = defineEvent(
  'PolkadotIngressEgress.UtxoConsolidation',
  polkadotIngressEgressUtxoConsolidation,
);
