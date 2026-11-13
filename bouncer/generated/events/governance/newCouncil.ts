import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceNewCouncil = z.object({ members: z.array(accountId) });

export const governanceNewCouncilEvent = defineEvent('Governance.NewCouncil', governanceNewCouncil);
