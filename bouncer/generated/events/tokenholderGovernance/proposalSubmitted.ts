import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tokenholderGovernanceProposalSubmitted = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});

export const tokenholderGovernanceProposalSubmittedEvent = defineEvent(
  'TokenholderGovernance.ProposalSubmitted',
  tokenholderGovernanceProposalSubmitted,
);
