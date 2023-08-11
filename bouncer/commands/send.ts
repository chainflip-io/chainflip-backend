#!/usr/bin/env -S pnpm tsx
import { Asset } from '@chainflip-io/cli';
import { send } from '../shared/send';

send(process.argv[2].toUpperCase() as Asset, process.argv[3]);
