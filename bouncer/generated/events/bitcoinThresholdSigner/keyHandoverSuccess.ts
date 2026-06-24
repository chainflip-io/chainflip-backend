import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });

export const bitcoinThresholdSignerKeyHandoverSuccessEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverSuccess',
  bitcoinThresholdSignerKeyHandoverSuccess,
);
