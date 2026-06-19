import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeygenFailure = numberOrHex;

export const bitcoinThresholdSignerKeygenFailureEvent = defineEvent(
  'BitcoinThresholdSigner.KeygenFailure',
  bitcoinThresholdSignerKeygenFailure,
);
