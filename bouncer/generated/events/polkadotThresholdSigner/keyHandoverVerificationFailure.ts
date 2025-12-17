import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});
