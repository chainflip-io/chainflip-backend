import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerNoKeyHandover = z.null();

export const solanaThresholdSignerNoKeyHandoverEvent = defineEvent(
  'SolanaThresholdSigner.NoKeyHandover',
  solanaThresholdSignerNoKeyHandover,
);
