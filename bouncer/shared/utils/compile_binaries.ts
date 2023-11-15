import { execSync } from 'child_process';

// Returns the expected next version of the runtime.
export async function compileBinaries(type: 'runtime' | 'all', projectRoot: string, tryRuntime = false) {
  if (type === 'all') {
    console.log('Building all the binaries...');
    if (tryRuntime) {
      execSync(`cd ${projectRoot} && cargo build --release --features try-runtime`);
    } else {
      execSync(`cd ${projectRoot} && cargo build --release`);
    }
  } else {
    console.log('Building the new runtime...');
    if (tryRuntime) {
      execSync(`cd ${projectRoot}/state-chain/runtime && cargo build --release`);
    } else {
      execSync(`cd ${projectRoot}/state-chain/runtime && cargo build --release --features try-runtime`);
    }
  }

  console.log('Build complete.');
}

try-runtime--runtime / Users / kylezs / Documents / cf - repos / chainflip - backend / target / debug / wbuild / state - chain - runtime / state_chain_runtime.wasm \
on - runtime - upgrade \
live--uri ws://localhost:9944

