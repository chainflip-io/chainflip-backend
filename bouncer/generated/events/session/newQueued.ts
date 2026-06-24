import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const sessionNewQueued = z.null();

export const sessionNewQueuedEvent = defineEvent('Session.NewQueued', sessionNewQueued);
