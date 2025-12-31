import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';

export const tokenholderGovernanceProposalEnacted = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});
