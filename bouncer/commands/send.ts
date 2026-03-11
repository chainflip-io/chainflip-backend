#!/usr/bin/env -S pnpm tsx
import { send } from 'shared/send';
import { parseAssetString, Asset } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';

await send(globalLogger, parseAssetString(process.argv[2]) as Asset, process.argv[3]);
