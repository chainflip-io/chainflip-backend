import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, hexString } from '../common';

export const tokenholderGovernanceGovKeyUpdatedHasFailed = z.object({
  chain: cfPrimitivesChainsForeignChain,
  key: hexString,
});
