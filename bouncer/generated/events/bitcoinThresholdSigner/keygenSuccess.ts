import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeygenSuccess = numberOrHex;

export const bitcoinThresholdSignerKeygenSuccessEvent = defineEvent(
  'BitcoinThresholdSigner.KeygenSuccess',
  bitcoinThresholdSignerKeygenSuccess,
);
