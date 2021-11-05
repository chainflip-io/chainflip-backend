# Chainflip Reputation Pallet

A module to manage the reputation of our validators for the Chainflip State Chain

## Overview

Validators earn *online credits* whilst they receive heartbeats.  These *online credits* are exchanged for *reputation points*
when they have been *online* for a specified period.  *Reputation points* buffer the validator from being slashed when they go offline for a period of time.

Penalties in terms of reputation points are incurred when any one of the *offline conditions* are
met.  Falling into negative reputation leads to the eventual slashing of FLIP.  As soon as reputation
is positive slashing stops.

In the event that we found a percentage, `EmergencyRotationPercentageTrigger`, of the validating set are offline then
an Emergency Rotation is requested via `EmergencyRotation::request_emergency_rotation()`

## Terminology
- Validator: A node in our network that is producing blocks.
- Heartbeat: A term used to measure the liveness of a validator.
- Online credits: A credit accrued by being continuously online which inturn is used to earn.
  *reputation points*.  Failing to stay *online* results in losing all of their *online credits*.
- Reputation points: A point system which allows validators to earn reputation by being *online*.
  They lose reputation points by being meeting one of the *offline conditions*.
- Offline conditions: One of the following conditions: *missed heartbeat*, *failed to broadcast
  an output*, *failed to participate in a signing ceremony*, *not enough performance credits* and
  *contradicting self during signing ceremony*.  Each condition has its associated penalty in
  reputation points.
- Slashing: The process of debiting FLIP tokens from a validator.  Slashing only occurs in this
  pallet when a validator's reputation points fall below zero *and* they are *offline*.
- Accrual Ratio: A ratio of reputation points earned per number of offline credits
