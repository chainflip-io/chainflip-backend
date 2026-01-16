import { z } from 'zod';
import { palletCfGovernanceGovernanceCouncil } from '../common';

export const governanceNewGovernanceCouncil = z.object({
  newCouncil: palletCfGovernanceGovernanceCouncil,
});
