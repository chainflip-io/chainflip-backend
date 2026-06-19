import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceGovKeyCallExecutionFailed = z.object({
  callHash: hexString,
  error: spRuntimeDispatchError,
});

export const governanceGovKeyCallExecutionFailedEvent = defineEvent(
  'Governance.GovKeyCallExecutionFailed',
  governanceGovKeyCallExecutionFailed,
);
