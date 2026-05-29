import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const grandpaGrandpaVoteDelegationRemoved = z.object({ delegator: hexString });

export const grandpaGrandpaVoteDelegationRemovedEvent = defineEvent(
  'Grandpa.GrandpaVoteDelegationRemoved',
  grandpaGrandpaVoteDelegationRemoved,
);
