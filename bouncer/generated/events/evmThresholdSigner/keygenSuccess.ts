import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeygenSuccess = numberOrHex;

export const evmThresholdSignerKeygenSuccessEvent = defineEvent(
  'EvmThresholdSigner.KeygenSuccess',
  evmThresholdSignerKeygenSuccess,
);
