import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemRemarked = z.object({ sender: accountId, hash_: hexString });

export const systemRemarkedEvent = defineEvent('System.Remarked', systemRemarked);
