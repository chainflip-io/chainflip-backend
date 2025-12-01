import { z } from 'zod';
import { hexString } from '../common';

export const environmentAssethubVaultAccountSet = z.object({ assethubVaultAccountId: hexString });
