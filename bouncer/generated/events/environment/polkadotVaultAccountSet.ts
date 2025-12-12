import { z } from 'zod';
import { hexString } from '../common';

export const environmentPolkadotVaultAccountSet = z.object({ polkadotVaultAccountId: hexString });
