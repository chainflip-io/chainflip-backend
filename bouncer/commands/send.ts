#!/usr/bin/env -S pnpm tsx
import { ChainflipAsset as InternalAsset } from '@chainflip/utils/chainflip';
import { send } from 'shared/send';
import { parseAssetString } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';

await send(globalLogger, parseAssetString(process.argv[2]) as InternalAsset, process.argv[3]);
