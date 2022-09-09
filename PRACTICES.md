# Justification

The intention of this document is to ensure the team makes decisions, work can move forward, and the team members are comfortable with those decisions, even when the members initially do not agree.

# Location

We choose to document our practices here inside this repository, so they are easily assessible and we can use Github's source control tools to manage the maintainence of this document.

# Requirements

Our practices must be:
- As unambiguous as possible. It should be clear how and when the practice applies. If necessary they should include clarifying examples. Ideally these examples would be from our codebase, and include links to PRs and issues.
- Well-reasoned. Otherwise we will likely alienate some developers. As such each practice should be justified using logic, principles, ideals, and other practices.

# Maintainence

Create an issue on this repository regarding the modification to / problem with the practices, provide a justification for the change, and add the <span style="color:rgb(0,255,0)">practices</span> label. We intentionally chose not to burden ourselves with a formal process to agree on modifications to the practices, but it is expected the whole team be given the opportinity to comment, a majority of the team should agree to a change, and those who disagreed are satisfied with the resolution. Once the modification is agreed on by the team, create a pull request to modify this document. That pull request should contain changes that ensure the codebase matches the new practices if applicable.

# Application

It is expected we "disagree and commit" with the contents of this document. If you feel a particular practice should be changed, you should still follow the practice until the team agrees to a change.

These practices should be used to help resolve disagreements. If that is not possible, the disagreement should be escalated so it can be resolved. If possible the resolution should result in modifcations to this document.

# Template

# Principles

These are the foundation of our practices...

## Story

When writing code you should aim to structure it in a manner that reflects how you would describe the solution.

### Example 1

In this [example](https://github.com/chainflip-io/chainflip-backend/pull/1505/commits/fa1f9099db2551ef2bf16d960a29ea624dd480fe) we introduced a `CallHashPrintable` type so we can print instances of `CallHash`. But in some places we still used `CallHash`, which confuses the story by making it seem that there is a conceptually important difference between the two structs. So [instead](https://github.com/chainflip-io/chainflip-backend/pull/1505/files#diff-7fe0fe870a6f0bf616e2a8cd94d959e9cc9bc0c8a0461b45618b0d51518e262cR74) we can `impl` `Debug` directly on `CallHash`, instead of introducing a separate type.

### Example 2

In this code `!iter::zip(call_hashes.iter(), call_hashes.iter().skip(1)).all(|(a, b)| a == b),` it is not immediately clear what the intent is. That code is from [here](https://github.com/chainflip-io/chainflip-backend/pull/1957/files/f0d7b9834ef9d00bd579d4017249f2b5659dac35..9aaa8dbcf7025e88ec31f3a9fed2b9de15589ea6#diff-2cd089f1f6104ab7dc556c4e1414300f65a28c3ce7bad521a9fadf1a6cd1c7cfR102), and as the comment below states we want to assert all the witness call hashes are equal after `extraction`. This is because `extraction` resolves any differences in witnesses (In this case via taking a median), and updates the call structures to contain the median. Therefore the hashes of the call structures should now all be the same as they all contain the same values.

Unfortunately there is not a `std::iter::Iterator::all_equal` utility, which would be the ideal way to express this check. But there are three uses of the same piece code in this function, each time being used to check if the elements in an iterator are all equal or not. We could therefore factor out a function:

```rust
fn all_equal<T: PartialEq, It: Iterator<Item = T>>(it: It) -> bool {
	use itertools::Itertools;
	it.windows(2).all(|(i, i_next)| i == i_next)
}

assert!(all_equal(call_hashes.iter()));
```

Which is dramatically clearer. And in fact a `all_same` function already existed in our codebase at this time.

Looking at the same code just above [here](https://github.com/chainflip-io/chainflip-backend/pull/1957/files/f0d7b9834ef9d00bd579d4017249f2b5659dac35..9aaa8dbcf7025e88ec31f3a9fed2b9de15589ea6#diff-2cd089f1f6104ab7dc556c4e1414300f65a28c3ce7bad521a9fadf1a6cd1c7cfR92):

```rust
let call_hashes = calls.iter().map(|call| CallHash(call.blake2_256())).collect::<Vec<_>>();
if !fees.iter().zip(fees.iter().skip(1)).all(|(a, b)| a == b) {
	assert!(
		!iter::zip(call_hashes.iter(), call_hashes.iter().skip(1)).all(|(a, b)| a == b),
		"Call hashes should be different before extraction if fees differ."
	)
}
```

As you can see this check checks that if some of the witness call's fees are different, that some of the call hashes are different. This check seems very strange even with the assert message because it is not checking a conceptually meaningful condition. The correct story here is, we want to check that if the fees of two different witness call's are different then their call hashes should also be different. You could express that like this:

```rust
let zip = iter::zip(fees.iter(), call_hashes.iter());
assert!(itertools::iproduct!(zip.clone(), zip).all(|((fee, call_hash), (other_fee, other_call_hash))| !((fee == other_fee) ^ (call_hash == other_call_hash))));
```

Although we could write this check in many different ways some possibly clearer than this, the important thing here is regardless of how it is written once I do understand what the check does, it will be much easier to understand why the check is done, as the check is meaningful.

# Ideals

# Practices

## Code

## Development Operations

### Submitting Code Changes

### Issues

### Release Process

# Code Reviews

The principles above are primarily to be applied by those submitting PRs. However, there are some principles that apply more specifically to when one is conducting a review of someone else's work.

Improving the speed and effectiveness of code reviews directly improves the speed and effectiveness of the team.

## Better not perfect

If a PR improves the overall quality of the code base, reviewers should tend towards approving over holding up the PR. Assuming there are no bugs introduced, this may mean approving with some comments, perhaps suggesting that some improvements be taken out into their own issue, or for even smaller items, allowing the developer to make a judgement call on whether to include it before merging. 