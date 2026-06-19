import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeygenFailure = numberOrHex;

export const evmThresholdSignerKeygenFailureEvent = defineEvent(
  'EvmThresholdSigner.KeygenFailure',
  evmThresholdSignerKeygenFailure,
);
