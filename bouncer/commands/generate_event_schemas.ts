#!/usr/bin/env -S pnpm tsx
import generate from '@chainflip/processor/generate';
import * as path from 'path';
import generateAggregatedEvents from './generate_aggregated_events';

const generatedDir = path.join(import.meta.dirname, '..', 'generated', 'events');

await generate(generatedDir);
await generateAggregatedEvents(generatedDir);
