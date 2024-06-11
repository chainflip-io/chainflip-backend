#!/usr/bin/env -S pnpm tsx
import { send } from '../shared/send';
import { parseAssetString } from '../shared/utils';

await send(parseAssetString(process.argv[2]), process.argv[3]);
