import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const sessionNewSession = z.object({ sessionIndex: z.number() });

export const sessionNewSessionEvent = defineEvent('Session.NewSession', sessionNewSession);
