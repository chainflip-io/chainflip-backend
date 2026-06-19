import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});

export const tronIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'TronIngressEgress.FailedToBuildAllBatchCall',
  tronIngressEgressFailedToBuildAllBatchCall,
);
