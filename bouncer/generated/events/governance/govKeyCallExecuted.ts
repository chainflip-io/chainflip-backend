import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceGovKeyCallExecuted = z.object({ callHash: hexString });

export const governanceGovKeyCallExecutedEvent = defineEvent(
  'Governance.GovKeyCallExecuted',
  governanceGovKeyCallExecuted,
);
