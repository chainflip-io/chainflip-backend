#!/usr/bin/env -S pnpm tsx
import generate from '@chainflip/processor/generate';
import * as path from 'path';

const generatedDir = path.join(import.meta.dirname, '..', 'generated', 'events');

await generate(generatedDir);