import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentAssethubVaultAccountSet = z.object({ assethubVaultAccountId: hexString });

export const environmentAssethubVaultAccountSetEvent = defineEvent(
  'Environment.AssethubVaultAccountSet',
  environmentAssethubVaultAccountSet,
);
