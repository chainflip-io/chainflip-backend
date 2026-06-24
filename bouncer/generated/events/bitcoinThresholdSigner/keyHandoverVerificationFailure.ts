import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});

export const bitcoinThresholdSignerKeyHandoverVerificationFailureEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverVerificationFailure',
  bitcoinThresholdSignerKeyHandoverVerificationFailure,
);
