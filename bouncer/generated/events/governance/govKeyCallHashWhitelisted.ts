import { z } from 'zod';
import { hexString } from '../common';

export const governanceGovKeyCallHashWhitelisted = z.object({ callHash: hexString });
