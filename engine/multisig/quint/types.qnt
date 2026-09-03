// Shared vocabulary for the multisig ceremony models.
//
// Mirrors engine/multisig/src/client/. Cryptography is abstract: a `Value`
// stands for whatever a stage broadcasts (a commitment, a complaint set),
// distinguishable only by equality.
module types {
  type Party = int
  type Value = int

  // What a party sent in a broadcast round, from a receiver's point of view.
  type Vote = Sent(Value) | Nothing

  // A round-1 send: `from` sent `vote` to `to`.
  type Send = { from: Party, to: Party, vote: Vote }

  // A round-2 claim: `by` tells `to` that it heard `vote` from `about`.
  type Claim = { by: Party, to: Party, about: Party, vote: Vote }

  // Result of find_frequent_element: the value more than `t` reporters agree on.
  type Opt = Found(Vote) | NoQuorum

  // Result of verify_broadcasts at one party.
  type Outcome =
    | Agreed(Party -> Vote)
    | Attributed(Set[Party])
    | Unattributed

  pure val PARTIES: Set[Party] = 1.to(4)
  pure val BYZ: Set[Party] = Set(4)
  pure val HONEST: Set[Party] = PARTIES.exclude(BYZ)
  pure val VALUES: Set[Value] = Set(0, 1)
  pure val VOTES: Set[Vote] = VALUES.map(v => Sent(v)).union(Set(Nothing))

  // threshold_from_share_count (utilities/src/lib.rs:72). Quorum is `count > t`.
  pure def thresholdOf(n: int): int = (2 * n - 1) / 3

  pure val T: int = thresholdOf(PARTIES.size())
}
