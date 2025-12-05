import { z } from 'zod';
import { numberOrHex, palletCfFlipImbalancesImbalanceSource } from '../common';

export const flipRemainingImbalance = z.object({
  who: palletCfFlipImbalancesImbalanceSource,
  remainingImbalance: numberOrHex,
});
