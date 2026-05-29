import { z } from 'zod';
import { accountId, cfChainsEvmAggKey, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerThresholdSignatureRequest = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  epoch: z.number(),
  key: cfChainsEvmAggKey,
  signatories: z.array(accountId),
  payload: hexString,
});

export const evmThresholdSignerThresholdSignatureRequestEvent = defineEvent(
  'EvmThresholdSigner.ThresholdSignatureRequest',
  evmThresholdSignerThresholdSignatureRequest,
);
