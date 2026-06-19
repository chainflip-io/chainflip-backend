import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeygenFailure = numberOrHex;

export const polkadotThresholdSignerKeygenFailureEvent = defineEvent(
  'PolkadotThresholdSigner.KeygenFailure',
  polkadotThresholdSignerKeygenFailure,
);
