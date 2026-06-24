import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tokenholderGovernanceProposalPassed = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});

export const tokenholderGovernanceProposalPassedEvent = defineEvent(
  'TokenholderGovernance.ProposalPassed',
  tokenholderGovernanceProposalPassed,
);
