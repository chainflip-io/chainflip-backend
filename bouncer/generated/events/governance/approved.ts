import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const governanceApproved = z.number();

export const governanceApprovedEvent = defineEvent('Governance.Approved', governanceApproved);
