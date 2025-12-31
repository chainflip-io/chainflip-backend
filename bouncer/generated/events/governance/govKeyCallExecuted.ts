import { z } from 'zod';
import { hexString } from '../common';

export const governanceGovKeyCallExecuted = z.object({ callHash: hexString });
