import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const evmThresholdSignerKeyHandoverVerificationSuccess = z.object({
  aggKey: cfChainsEvmAggKey,
});
