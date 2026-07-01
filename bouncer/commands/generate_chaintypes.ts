#!/usr/bin/env -S pnpm tsx
// Regenerates the dedot chain types (the `ChainflipNodeApi` interface used for
// compile-time-typed extrinsics and queries) from a running node's metadata.
// Mirrors generate_event_schemas.ts and is invoked from localnet/common.sh on boot so
// the types track the runtime automatically. Requires a reachable node.
import { execFileSync } from 'node:child_process';
import * as path from 'path';

const endpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
const outDir = path.join(import.meta.dirname, '..', 'generated', 'chaintypes');

execFileSync('npx', ['dedot', 'chaintypes', '-w', endpoint, '-o', outDir], {
  stdio: 'inherit',
});
