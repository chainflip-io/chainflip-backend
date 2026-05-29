import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});

export const solanaIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'SolanaIngressEgress.FailedToBuildAllBatchCall',
  solanaIngressEgressFailedToBuildAllBatchCall,
);
