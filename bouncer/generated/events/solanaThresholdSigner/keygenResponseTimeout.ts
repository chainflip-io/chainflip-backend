import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeygenResponseTimeout = numberOrHex;

export const solanaThresholdSignerKeygenResponseTimeoutEvent = defineEvent(
  'SolanaThresholdSigner.KeygenResponseTimeout',
  solanaThresholdSignerKeygenResponseTimeout,
);
