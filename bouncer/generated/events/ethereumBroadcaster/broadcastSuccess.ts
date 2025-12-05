import { z } from 'zod';
import { cfChainsEvmSchnorrVerificationComponents, hexString } from '../common';

export const ethereumBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
  transactionRef: hexString,
});
