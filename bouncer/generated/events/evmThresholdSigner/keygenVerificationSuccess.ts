import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';

export const evmThresholdSignerKeygenVerificationSuccess = z.object({ aggKey: cfChainsEvmAggKey });
