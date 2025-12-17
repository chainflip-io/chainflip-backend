import { z } from 'zod';
import { palletCfTokenholderGovernanceProposal } from '../common';

export const tokenholderGovernanceProposalRejected = z.object({
  proposal: palletCfTokenholderGovernanceProposal,
});
