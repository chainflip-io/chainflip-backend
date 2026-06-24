import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });

export const evmThresholdSignerKeyHandoverFailureEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverFailure',
  evmThresholdSignerKeyHandoverFailure,
);
