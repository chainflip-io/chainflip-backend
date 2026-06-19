import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeygenResponseTimeout = numberOrHex;

export const bitcoinThresholdSignerKeygenResponseTimeoutEvent = defineEvent(
  'BitcoinThresholdSigner.KeygenResponseTimeout',
  bitcoinThresholdSignerKeygenResponseTimeout,
);
