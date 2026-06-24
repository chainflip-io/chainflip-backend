import { z } from 'zod';
import { palletCfGovernanceGovernanceCouncil } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceNewGovernanceCouncil = z.object({
  newCouncil: palletCfGovernanceGovernanceCouncil,
});

export const governanceNewGovernanceCouncilEvent = defineEvent(
  'Governance.NewGovernanceCouncil',
  governanceNewGovernanceCouncil,
);
