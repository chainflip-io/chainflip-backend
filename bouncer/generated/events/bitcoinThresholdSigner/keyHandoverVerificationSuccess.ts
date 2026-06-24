import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverVerificationSuccess = z.object({
  aggKey: cfChainsBtcAggKey,
});

export const bitcoinThresholdSignerKeyHandoverVerificationSuccessEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverVerificationSuccess',
  bitcoinThresholdSignerKeyHandoverVerificationSuccess,
);
