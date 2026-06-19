import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverResponseTimeout = z.object({ ceremonyId: numberOrHex });

export const evmThresholdSignerKeyHandoverResponseTimeoutEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverResponseTimeout',
  evmThresholdSignerKeyHandoverResponseTimeout,
);
