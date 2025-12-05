import { z } from 'zod';
import { cfChainsSolApiSolanaGovCall } from '../common';

export const environmentSolanaGovCallDispatched = z.object({
  govCall: cfChainsSolApiSolanaGovCall,
  broadcastId: z.number(),
});
