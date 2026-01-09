import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});
