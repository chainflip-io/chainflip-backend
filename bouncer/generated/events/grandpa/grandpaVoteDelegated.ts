import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const grandpaGrandpaVoteDelegated = z.object({ delegator: hexString, delegate: hexString });

export const grandpaGrandpaVoteDelegatedEvent = defineEvent(
  'Grandpa.GrandpaVoteDelegated',
  grandpaGrandpaVoteDelegated,
);
