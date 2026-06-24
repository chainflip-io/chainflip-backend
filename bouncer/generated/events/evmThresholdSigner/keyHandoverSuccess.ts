import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });

export const evmThresholdSignerKeyHandoverSuccessEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverSuccess',
  evmThresholdSignerKeyHandoverSuccess,
);
