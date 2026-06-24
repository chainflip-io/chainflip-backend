import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyHandoverResponseTimeout = z.object({
  ceremonyId: numberOrHex,
});

export const polkadotThresholdSignerKeyHandoverResponseTimeoutEvent = defineEvent(
  'PolkadotThresholdSigner.KeyHandoverResponseTimeout',
  polkadotThresholdSignerKeyHandoverResponseTimeout,
);
