import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});
