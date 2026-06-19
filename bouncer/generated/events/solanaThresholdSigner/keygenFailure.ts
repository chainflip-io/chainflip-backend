import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeygenFailure = numberOrHex;

export const solanaThresholdSignerKeygenFailureEvent = defineEvent(
  'SolanaThresholdSigner.KeygenFailure',
  solanaThresholdSignerKeygenFailure,
);
