# Chainflip Alive Module

A module to manage liveness for the Chainflip State Chain

- [`Config`]
- [`Call`]
- [`Module`]

## Overview

The module contains functionality to track behaviour of accounts and provides a good indication
of an account's liveliness.  The rules determining what is good or bad behaviour is outside the
scope of this pallet and this pallet is solely responsible in tracking and storing the
behavioural data. Actions, or behaviours, are stored and indexed by the account id of the
validator. The last behaviour recorded for a validator would be used as its last know 'live'
time and hence serve as a strong indicator of its liveliness in terms of an operational node.
In order to prevent spamming a whitelist of accounts is controlled in which before reporting
behaviour for an account the account has to be explicitly added using `add_account()` and
removed with `remove_account()`.  Liveliness is stored separately, in the `LastKnownLiveliness`
storage map, from the tracked behaviour to maintain this indicator after cleaning the
behavioural data on an account.

## Terminology

- **Liveness:** - the last block number we have had a report on an account for
