import { execSync } from 'child_process';


// TODO: Maybe only compile with try-runtime if we need to.
export async function compileBinaries(type: 'runtime' | 'all', projectRoot: string) {
  if (type === 'all') {
    console.log('Building all the binaries...');
    execSync(`cd ${projectRoot} && cargo build --release --features try-runtime`);
  } else {
    console.log('Building the new runtime...');
    execSync(`cd ${projectRoot}/state-chain/runtime && cargo build --release --features try-runtime`);
  }

  console.log('Build complete.');
}
