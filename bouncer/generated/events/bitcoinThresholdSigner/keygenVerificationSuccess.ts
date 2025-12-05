import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';

export const bitcoinThresholdSignerKeygenVerificationSuccess = z.object({
  aggKey: cfChainsBtcAggKey,
});
