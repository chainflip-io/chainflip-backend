import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});
