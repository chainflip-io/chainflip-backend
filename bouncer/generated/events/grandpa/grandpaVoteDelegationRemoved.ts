import { z } from 'zod';
import { hexString } from '../common';

export const grandpaGrandpaVoteDelegationRemoved = z.object({ delegator: hexString });
