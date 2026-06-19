import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const governanceExecuted = z.number();

export const governanceExecutedEvent = defineEvent('Governance.Executed', governanceExecuted);
