import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerNoKeyHandover = z.null();

export const polkadotThresholdSignerNoKeyHandoverEvent = defineEvent(
  'PolkadotThresholdSigner.NoKeyHandover',
  polkadotThresholdSignerNoKeyHandover,
);
