import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const grandpaPaused = z.null();

export const grandpaPausedEvent = defineEvent('Grandpa.Paused', grandpaPaused);
