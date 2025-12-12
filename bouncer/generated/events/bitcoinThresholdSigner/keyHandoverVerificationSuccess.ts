import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';

export const bitcoinThresholdSignerKeyHandoverVerificationSuccess = z.object({
  aggKey: cfChainsBtcAggKey,
});
