import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceNewVotingAuthority = z.object({ members: z.array(accountId) });

export const governanceNewVotingAuthorityEvent = defineEvent(
  'Governance.NewVotingAuthority',
  governanceNewVotingAuthority,
);
