import { z } from 'zod';
import { numberOrHex } from '../common';

export const ethereumChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});
