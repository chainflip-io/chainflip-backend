import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemKilledAccount = z.object({ account: accountId });

export const systemKilledAccountEvent = defineEvent('System.KilledAccount', systemKilledAccount);
