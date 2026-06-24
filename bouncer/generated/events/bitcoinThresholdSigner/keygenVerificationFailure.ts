import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});

export const bitcoinThresholdSignerKeygenVerificationFailureEvent = defineEvent(
  'BitcoinThresholdSigner.KeygenVerificationFailure',
  bitcoinThresholdSignerKeygenVerificationFailure,
);
