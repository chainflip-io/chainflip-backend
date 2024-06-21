#!/usr/bin/env -S pnpm tsx
import { InternalAsset } from '@chainflip/cli';
import { send } from '../shared/send';
import { parseAssetString } from '../shared/utils';

await send(parseAssetString(process.argv[2]) as InternalAsset, process.argv[3]);
