import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeygenVerificationSuccess = z.object({
  aggKey: cfChainsBtcAggKey,
});

export const bitcoinThresholdSignerKeygenVerificationSuccessEvent = defineEvent(
  'BitcoinThresholdSigner.KeygenVerificationSuccess',
  bitcoinThresholdSignerKeygenVerificationSuccess,
);
