import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeygenSuccess = numberOrHex;

export const solanaThresholdSignerKeygenSuccessEvent = defineEvent(
  'SolanaThresholdSigner.KeygenSuccess',
  solanaThresholdSignerKeygenSuccess,
);
