import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverVerificationFailure = z.object({
  handoverCeremonyId: numberOrHex,
});

export const evmThresholdSignerKeyHandoverVerificationFailureEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverVerificationFailure',
  evmThresholdSignerKeyHandoverVerificationFailure,
);
