import { z } from 'zod';
import { spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceFailedExecution = spRuntimeDispatchError;

export const governanceFailedExecutionEvent = defineEvent(
  'Governance.FailedExecution',
  governanceFailedExecution,
);
