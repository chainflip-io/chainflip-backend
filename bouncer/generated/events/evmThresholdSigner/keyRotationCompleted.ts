import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyRotationCompleted = z.null();

export const evmThresholdSignerKeyRotationCompletedEvent = defineEvent(
  'EvmThresholdSigner.KeyRotationCompleted',
  evmThresholdSignerKeyRotationCompleted,
);
