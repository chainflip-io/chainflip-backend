import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeygenResponseTimeout = numberOrHex;

export const polkadotThresholdSignerKeygenResponseTimeoutEvent = defineEvent(
  'PolkadotThresholdSigner.KeygenResponseTimeout',
  polkadotThresholdSignerKeygenResponseTimeout,
);
