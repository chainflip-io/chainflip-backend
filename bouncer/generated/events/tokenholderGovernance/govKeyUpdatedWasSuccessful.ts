import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tokenholderGovernanceGovKeyUpdatedWasSuccessful = z.object({
  chain: cfPrimitivesChainsForeignChain,
  key: hexString,
});

export const tokenholderGovernanceGovKeyUpdatedWasSuccessfulEvent = defineEvent(
  'TokenholderGovernance.GovKeyUpdatedWasSuccessful',
  tokenholderGovernanceGovKeyUpdatedWasSuccessful,
);
