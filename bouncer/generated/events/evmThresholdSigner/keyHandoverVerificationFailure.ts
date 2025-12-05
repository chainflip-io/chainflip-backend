import { z } from 'zod';
import { numberOrHex } from '../common';

export const evmThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});
