import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerThresholdSignatureRequest = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  epoch: z.number(),
  key: hexString,
  signatories: z.array(accountId),
  payload: hexString,
});

export const polkadotThresholdSignerThresholdSignatureRequestEvent = defineEvent(
  'PolkadotThresholdSigner.ThresholdSignatureRequest',
  polkadotThresholdSignerThresholdSignatureRequest,
);
