import { z } from 'zod';
import { numberOrHex, palletCfFlipImbalancesImbalanceSource } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const flipRemainingImbalance = z.object({
  who: palletCfFlipImbalancesImbalanceSource,
  remainingImbalance: numberOrHex,
});

export const flipRemainingImbalanceEvent = defineEvent(
  'Flip.RemainingImbalance',
  flipRemainingImbalance,
);
