#!/usr/bin/env -S pnpm tsx
import { InternalAsset as Asset } from '@chainflip/cli';
import { send } from '../shared/send';

send(process.argv[2] as Asset, process.argv[3]);
