#!/bin/bash
pnpm vitest --maxConcurrency=1000 run -t "ConcurrentTests"