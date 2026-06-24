import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentPolkadotVaultAccountSet = z.object({ polkadotVaultAccountId: hexString });

export const environmentPolkadotVaultAccountSetEvent = defineEvent(
  'Environment.PolkadotVaultAccountSet',
  environmentPolkadotVaultAccountSet,
);
