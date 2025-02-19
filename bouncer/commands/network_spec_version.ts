#!/usr/bin/env -S pnpm tsx

import { getNetworkRuntimeVersion } from '../shared/utils/spec_version';

const network = process.argv[2];

const runtimeVersion = await getNetworkRuntimeVersion(network);

console.log(runtimeVersion.specVersion);
