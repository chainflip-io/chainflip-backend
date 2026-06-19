import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeygenVerificationFailure = z.object({
  keygenCeremonyId: numberOrHex,
});

export const evmThresholdSignerKeygenVerificationFailureEvent = defineEvent(
  'EvmThresholdSigner.KeygenVerificationFailure',
  evmThresholdSignerKeygenVerificationFailure,
);
