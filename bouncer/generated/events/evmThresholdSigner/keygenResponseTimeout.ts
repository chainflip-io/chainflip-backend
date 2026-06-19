import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeygenResponseTimeout = numberOrHex;

export const evmThresholdSignerKeygenResponseTimeoutEvent = defineEvent(
  'EvmThresholdSigner.KeygenResponseTimeout',
  evmThresholdSignerKeygenResponseTimeout,
);
