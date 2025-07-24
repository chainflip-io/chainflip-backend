#!/bin/bash
pnpm vitest --maxConcurrency=100 run -t "ConcurrentTests"