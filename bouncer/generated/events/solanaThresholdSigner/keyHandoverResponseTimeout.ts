import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaThresholdSignerKeyHandoverResponseTimeout = z.object({
  ceremonyId: numberOrHex,
});
