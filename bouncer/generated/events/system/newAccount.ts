import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemNewAccount = z.object({ account: accountId });

export const systemNewAccountEvent = defineEvent('System.NewAccount', systemNewAccount);
