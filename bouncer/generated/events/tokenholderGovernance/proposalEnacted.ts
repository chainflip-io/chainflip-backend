import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tokenholderGovernanceProposalEnacted = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});

export const tokenholderGovernanceProposalEnactedEvent = defineEvent(
  'TokenholderGovernance.ProposalEnacted',
  tokenholderGovernanceProposalEnacted,
);
