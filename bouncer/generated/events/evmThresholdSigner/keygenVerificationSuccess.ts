import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeygenVerificationSuccess = z.object({ aggKey: cfChainsEvmAggKey });

export const evmThresholdSignerKeygenVerificationSuccessEvent = defineEvent(
  'EvmThresholdSigner.KeygenVerificationSuccess',
  evmThresholdSignerKeygenVerificationSuccess,
);
