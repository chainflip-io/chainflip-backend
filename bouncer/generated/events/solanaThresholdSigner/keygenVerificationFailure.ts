import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});

export const solanaThresholdSignerKeygenVerificationFailureEvent = defineEvent(
  'SolanaThresholdSigner.KeygenVerificationFailure',
  solanaThresholdSignerKeygenVerificationFailure,
);
