import { z } from 'zod';
import { cfChainsEvmSchnorrVerificationComponents, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
  transactionRef: hexString,
});

export const ethereumBroadcasterBroadcastSuccessEvent = defineEvent(
  'EthereumBroadcaster.BroadcastSuccess',
  ethereumBroadcasterBroadcastSuccess,
);
