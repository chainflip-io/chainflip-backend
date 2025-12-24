import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});
