import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemCodeUpdated = z.object({ hash_: hexString });

export const systemCodeUpdatedEvent = defineEvent('System.CodeUpdated', systemCodeUpdated);
