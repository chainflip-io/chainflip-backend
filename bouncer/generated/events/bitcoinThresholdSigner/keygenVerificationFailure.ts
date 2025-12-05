import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});
