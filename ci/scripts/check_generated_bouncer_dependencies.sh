#!/bin/bash

set -euo pipefail

./chainflip-node --dev &

# Wait for node to start
echo -e "🚀 Starting chainflip-node..."
sleep 30

#################################################
# Subsquid ingester event schemas

# Call script to generate events
echo -e "Generating event schemas..."
cd bouncer && ./commands/generate_event_schemas.ts
cd ..

# Check whether the event subdirectory is dirty
EVENTS_DIR="bouncer/generated/events"

if [[ -n "$(git status --porcelain -- "$EVENTS_DIR")" ]]; then
  echo "ERROR: Event schemas in '$EVENTS_DIR' are not up to date! Please run ./commands/generate_event_schemas.ts to regenerate schemas and commit them."
  echo ""
  echo "The following schema changes have not been committed:"
  echo ""
  git status -- "$EVENTS_DIR"
  exit 1
fi

#################################################
# Dedot substrate api chaintypes

# Call script to generate chaintypes (dedot)
echo -e "Generating chaintypes (dedot)..."
cd bouncer && ./commands/generate_chaintypes.ts
cd ..

# Check whether the chaintypes subdirectory is dirty
CHAINTYPES_DIR="bouncer/generated/chaintypes"

if [[ -n "$(git status --porcelain -- "$CHAINTYPES_DIR")" ]]; then
  echo "ERROR: Chaintypes in '$CHAINTYPES_DIR' are not up to date! Please run ./commands/generate_chaintypes.ts to regenerate dedot chaintypes and commit them."
  echo ""
  echo "The following chaintype changes have not been committed:"
  echo ""
  git status -- "$CHAINTYPES_DIR"
  exit 1
fi

#################################################
# Success

echo -e "Events and chaintypes are up to date!"