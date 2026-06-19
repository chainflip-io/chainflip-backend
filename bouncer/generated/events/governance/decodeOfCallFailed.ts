import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const governanceDecodeOfCallFailed = z.number();

export const governanceDecodeOfCallFailedEvent = defineEvent(
  'Governance.DecodeOfCallFailed',
  governanceDecodeOfCallFailed,
);
