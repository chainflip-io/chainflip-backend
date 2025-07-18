#!/bin/bash
pnpm vitest --maxConcurrency=500 run -t "ConcurrentTests"