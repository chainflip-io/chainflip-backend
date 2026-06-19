import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverResponseTimeout = z.object({
  ceremonyId: numberOrHex,
});

export const bitcoinThresholdSignerKeyHandoverResponseTimeoutEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverResponseTimeout',
  bitcoinThresholdSignerKeyHandoverResponseTimeout,
);
