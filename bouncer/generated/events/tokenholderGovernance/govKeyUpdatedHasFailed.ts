import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tokenholderGovernanceGovKeyUpdatedHasFailed = z.object({
  chain: cfPrimitivesChainsForeignChain,
  key: hexString,
});

export const tokenholderGovernanceGovKeyUpdatedHasFailedEvent = defineEvent(
  'TokenholderGovernance.GovKeyUpdatedHasFailed',
  tokenholderGovernanceGovKeyUpdatedHasFailed,
);
