---
name: consolidating-dependabot-prs
description: Use when asked to consolidate, batch, combine, or merge open dependabot PRs in chainflip-backend, or when dependabot PRs are open and CI hasn't run on them. Triggered by "consolidate the dependabot PRs", "batch the dependabot bumps", "dependabot PRs can't run CI".
---

# Consolidating Dependabot PRs

Dependabot's PRs can't access the secrets CI needs, so they need to be re-created under a team member's authority: one branch per batch, every original commit preserved and re-signed.

**Preserve the individual dependabot commits.** Squash only if the user asks for a single commit in that request; otherwise one commit per bump.

## Procedure

1. **List and fetch.** `gh pr list --author app/dependabot --json number,headRefName,title`, then `git fetch origin main <head refs>`.
2. **Find the base.** Check for an open PR whose head matches `chore/dependabot-updates-*`
   (`gh pr list --search 'head:chore/dependabot-updates' --json number,headRefName`).
   If one exists, the new branch is stacked on it and the PR targets it. Otherwise base on `origin/main`.
3. **Branch:** `chore/dependabot-updates-yy-mm-dd` using today's date (e.g. `chore/dependabot-updates-26-09-07`).
4. **Cherry-pick each commit separately**, oldest PR first, then add co-author trailers:
    ```bash
    git cherry-pick -S <sha>
    git commit --amend --no-edit -S \
      --trailer "Co-authored-by: $(git config user.name) <$(git config user.email)>" \
      --trailer "Co-authored-by: Claude Fable 5.1 <noreply@anthropic.com>"
    ```
    Dependabot stays the author; the user is committer and signer.
5. **Conflicts** are almost always the same action bumped twice (earlier batch vs. this one).
   Take the newer version (`git checkout --theirs <file>`), confirm with grep, continue.
6. **Verify:** `git log --format='%h %G? %an/%cn | %s' <base>..HEAD` shows `G dependabot[bot]` on every line.
7. **Push and open the PR.** Title `chore: dependabot updates yy-mm-dd`, base from step 2. Body:

    ```
    # Pull Request

    Consolidates recent dependabot PRs: #A #B #C

    PRs opened by the bot don't have access to the right secrets to be able to run CI.

    [Stacked on #N (`chore/dependabot-updates-yy-mm-dd`).]   <- only if stacked
    ```

    plus an Action / From / To table.

8. **Close each original** with `gh pr close <n> --comment "Superseded by #<new>."`.

## If signing fails

`git commit -S` fails with "signing failed: Operation cancelled" when the GPG agent has no
cached passphrase. Do not use `--no-gpg-sign`. Stop with the cherry-pick staged, write the
command to run, and hand it to the user to run in their own shell. Trailers use `git config user.name`
and `git config user.email`, not any other address. Sometimes the agent is warm and
signing just works; verify with `%G?` either way.

## Common Mistakes

| Mistake                                                  | Fix                                             |
| -------------------------------------------------------- | ----------------------------------------------- |
| One squashed commit "to keep it simple"                  | Cherry-pick each; history per bump is the point |
| Inventing a branch name (`chore/consolidate-...`)        | Always `chore/dependabot-updates-yy-mm-dd`      |
| Basing on `main` while an earlier batch PR is still open | Stack on that branch and target it              |
| Resolving a version conflict toward the older pin        | Take the newer version, then grep to confirm    |
| Deleting the pushed branch to rename it                  | Close the old PR with a pointer to the new one  |
