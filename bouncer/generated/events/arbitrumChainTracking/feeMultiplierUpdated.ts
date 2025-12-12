import { z } from 'zod';
import { numberOrHex } from '../common';

export const arbitrumChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});
