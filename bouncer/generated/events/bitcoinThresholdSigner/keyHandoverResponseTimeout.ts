import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinThresholdSignerKeyHandoverResponseTimeout = z.object({
  ceremonyId: numberOrHex,
});
