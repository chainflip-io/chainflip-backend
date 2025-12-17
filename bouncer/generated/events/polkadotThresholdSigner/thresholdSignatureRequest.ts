import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';

export const polkadotThresholdSignerThresholdSignatureRequest = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  epoch: z.number(),
  key: hexString,
  signatories: z.array(accountId),
  payload: hexString,
});
