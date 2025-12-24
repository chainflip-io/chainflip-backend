import { z } from 'zod';
import { numberOrHex } from '../common';

export const assethubChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});
