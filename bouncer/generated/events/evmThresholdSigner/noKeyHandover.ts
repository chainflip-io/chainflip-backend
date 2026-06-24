import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerNoKeyHandover = z.null();

export const evmThresholdSignerNoKeyHandoverEvent = defineEvent(
  'EvmThresholdSigner.NoKeyHandover',
  evmThresholdSignerNoKeyHandover,
);
