import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const governanceExpired = z.number();

export const governanceExpiredEvent = defineEvent('Governance.Expired', governanceExpired);
