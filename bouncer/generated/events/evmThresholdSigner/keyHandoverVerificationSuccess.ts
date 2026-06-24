import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverVerificationSuccess = z.object({
  aggKey: cfChainsEvmAggKey,
});

export const evmThresholdSignerKeyHandoverVerificationSuccessEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverVerificationSuccess',
  evmThresholdSignerKeyHandoverVerificationSuccess,
);
