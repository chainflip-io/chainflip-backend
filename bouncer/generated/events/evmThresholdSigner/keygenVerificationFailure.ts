import { z } from 'zod';
import { numberOrHex } from '../common';

export const evmThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});
