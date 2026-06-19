import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const systemCodeUpdated = z.null();

export const systemCodeUpdatedEvent = defineEvent('System.CodeUpdated', systemCodeUpdated);
