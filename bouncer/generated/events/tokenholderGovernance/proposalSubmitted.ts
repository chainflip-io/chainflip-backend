import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';

export const tokenholderGovernanceProposalSubmitted = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});
