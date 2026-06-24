import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerNoKeyHandover = z.null();

export const bitcoinThresholdSignerNoKeyHandoverEvent = defineEvent(
  'BitcoinThresholdSigner.NoKeyHandover',
  bitcoinThresholdSignerNoKeyHandover,
);
