import { z } from 'zod';
import { cfChainsBtcUtxo } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentStaleUtxosDiscarded = z.object({ utxos: z.array(cfChainsBtcUtxo) });

export const environmentStaleUtxosDiscardedEvent = defineEvent(
  'Environment.StaleUtxosDiscarded',
  environmentStaleUtxosDiscarded,
);
