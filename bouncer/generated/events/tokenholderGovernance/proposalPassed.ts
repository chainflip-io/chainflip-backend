import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';

export const tokenholderGovernanceProposalPassed = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});
