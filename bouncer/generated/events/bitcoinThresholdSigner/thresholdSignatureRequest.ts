import { z } from 'zod';
import {
  accountId,
  cfChainsBtcAggKey,
  cfChainsBtcPreviousOrCurrent,
  hexString,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerThresholdSignatureRequest = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  epoch: z.number(),
  key: cfChainsBtcAggKey,
  signatories: z.array(accountId),
  payload: z.array(z.tuple([cfChainsBtcPreviousOrCurrent, hexString])),
});

export const bitcoinThresholdSignerThresholdSignatureRequestEvent = defineEvent(
  'BitcoinThresholdSigner.ThresholdSignatureRequest',
  bitcoinThresholdSignerThresholdSignatureRequest,
);
