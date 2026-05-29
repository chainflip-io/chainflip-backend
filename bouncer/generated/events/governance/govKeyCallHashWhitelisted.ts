import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceGovKeyCallHashWhitelisted = z.object({ callHash: hexString });

export const governanceGovKeyCallHashWhitelistedEvent = defineEvent(
  'Governance.GovKeyCallHashWhitelisted',
  governanceGovKeyCallHashWhitelisted,
);
