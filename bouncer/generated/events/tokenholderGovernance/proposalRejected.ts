import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tokenholderGovernanceProposalRejected = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});

export const tokenholderGovernanceProposalRejectedEvent = defineEvent(
  'TokenholderGovernance.ProposalRejected',
  tokenholderGovernanceProposalRejected,
);
