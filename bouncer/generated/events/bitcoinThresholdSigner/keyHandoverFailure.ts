import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });

export const bitcoinThresholdSignerKeyHandoverFailureEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverFailure',
  bitcoinThresholdSignerKeyHandoverFailure,
);
