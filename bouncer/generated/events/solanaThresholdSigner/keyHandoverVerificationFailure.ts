import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});

export const solanaThresholdSignerKeyHandoverVerificationFailureEvent = defineEvent(
  'SolanaThresholdSigner.KeyHandoverVerificationFailure',
  solanaThresholdSignerKeyHandoverVerificationFailure,
);
