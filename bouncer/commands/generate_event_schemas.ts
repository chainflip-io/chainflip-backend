#!/usr/bin/env -S pnpm tsx
import generate from '@chainflip/processor/generate';
import * as path from 'path';
import generateGenericEvents from 'commands/generate_generic_events';

const generatedDir = path.join(import.meta.dirname, '..', 'generated', 'events');

await generate(generatedDir);
await generateGenericEvents(generatedDir);
