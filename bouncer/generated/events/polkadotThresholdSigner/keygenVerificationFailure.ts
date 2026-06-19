import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});

export const polkadotThresholdSignerKeygenVerificationFailureEvent = defineEvent(
  'PolkadotThresholdSigner.KeygenVerificationFailure',
  polkadotThresholdSignerKeygenVerificationFailure,
);
