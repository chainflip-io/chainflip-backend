import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeygenSuccess = numberOrHex;

export const polkadotThresholdSignerKeygenSuccessEvent = defineEvent(
  'PolkadotThresholdSigner.KeygenSuccess',
  polkadotThresholdSignerKeygenSuccess,
);
