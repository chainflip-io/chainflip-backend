import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const grandpaResumed = z.null();

export const grandpaResumedEvent = defineEvent('Grandpa.Resumed', grandpaResumed);
