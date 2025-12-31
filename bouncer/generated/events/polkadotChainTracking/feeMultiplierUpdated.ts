import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotChainTrackingFeeMultiplierUpdated = z.object({
  newFeeMultiplier: numberOrHex,
});
