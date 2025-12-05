import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotThresholdSignerKeyHandoverResponseTimeout = z.object({
  ceremonyId: numberOrHex,
});
