import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});

export const polkadotThresholdSignerKeyHandoverVerificationFailureEvent = defineEvent(
  'PolkadotThresholdSigner.KeyHandoverVerificationFailure',
  polkadotThresholdSignerKeyHandoverVerificationFailure,
);
