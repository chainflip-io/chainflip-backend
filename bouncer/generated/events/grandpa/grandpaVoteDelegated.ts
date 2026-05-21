import { z } from 'zod';
import { hexString } from '../common';

export const grandpaGrandpaVoteDelegated = z.object({ delegator: hexString, delegate: hexString });
