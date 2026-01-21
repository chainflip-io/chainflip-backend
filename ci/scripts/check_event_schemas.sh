#!/bin/bash

set -euo pipefail

./chainflip-node --dev &

# Wait for node to start
echo -e "🚀 Starting chainflip-node..."
sleep 10

# Call script to generate events
echo -e "Generating event schemas..."
cd bouncer && ./commands/generate_event_schemas.ts
cd ..

# Check whether a specific subdirectory is dirty
EVENTS_DIR="bouncer/generated/events"

if [[ -n "$(git status --porcelain -- "$EVENTS_DIR")" ]]; then
  echo "ERROR: Event schemas in '$EVENTS_DIR' are not up to date! Please run ./commands/generate_event_schemas.ts to regenerate schemas and commit them."
  echo ""
  echo "The following schema changes have not been comitted:"
  echo ""
  git status -- "$EVENTS_DIR"
  exit 1
fi

echo -e "Events are up to date!"