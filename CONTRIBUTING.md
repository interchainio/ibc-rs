# Contributing

Thank you for your interest in contributing to the Hermes IBC relayer! The goal
of this project is to provide a high quality, formally verified IBC relayer 
implementation in Rust.

All work on the code base should be motivated by a Github
issue. Before opening a new issue, first do a search of open and closed issues
to make sure that yours will not be a duplicate.
If you would like to work on an issue which already exists, please indicate so
by leaving a comment on the issue. If what you'd like to work on hasn't already 
been covered by an issue, then open a new one to get the process going. Please
do your best to ensure that your issue adheres to one of the 
[issue templates](https://github.com/informalsystems/hermes/tree/master/.github/ISSUE_TEMPLATE)
that we have in the repository. 

The rest of this document outlines the best practices for contributing to this
repository:

- [Core Team Responsibility](#responsibility) - what the core team is responsible for
- [Decision Making](#decision-making) - process for agreeing to changes
- [Issues](#issues) - what makes a good issue
- [Pull Requests](#pull-requests) - what makes a good pull request
- [Forking](#forking) - fork the repo to make pull requests
- [Changelog](#changelog) - changes must be recorded in the changelog
- [Releases](#releases) - how to release new version of the crates

## Core Team Responsibility

The Hermes core team is responsible for stewarding this project over time. This means that the core team needs to understand the nature of, and agree to maintain, all of the changes that land on the master branch. It may cost a few days or weeks' worth of time to submit a particular change, but maintaining that change over the years has a much higher cost that the core team is responsible for bearing.

With that in mind, the core team must balance the potential risks of maintaining 
any contribution in the long-term against the immediate usefulness or utility 
that contribution manifests. 

## Decision Making

When contributing to the project, the following process leads to the best chance of
landing the changes in `master`.

All new contributions should start with a Github issue which captures the
problem you're trying to solve. Starting off with an issue allows for early
feedback. Once the issue is created, maintainers may request that more detailed
documentation be written in the form of a Request for Comment (RFC) or an
Architectural Decision Record
([ADR](https://github.com/informalsystems/hermes/blob/master/docs/architecture/README.md)).

Discussion at the RFC stage will build collective understanding of the dimensions
of the problem and help structure conversations around trade-offs.

When the problem is well understood but the solution leads to large
structural changes to the code base, these changes should be proposed in
the form of an [Architectural Decision Record
(ADR)](./docs/architecture/). The ADR will help build consensus on an
overall strategy to ensure that the code base maintains coherence
in the larger context. If you are not comfortable with writing an ADR,
you can open a regular issue and the maintainers will help you
turn it into an ADR.

When the problem and the proposed solution are well understood,
changes should start with a [draft
pull request](https://github.blog/2019-02-14-introducing-draft-pull-requests/)
against `master`. The draft status signals that work is underway. When the work
is ready for feedback, hitting "Ready for Review" will signal to the
maintainers to take a look.

Implementation trajectories should aim to proceed where possible as a series
of smaller incremental changes, in the form of small PRs that can be merged
quickly. This helps manage the load for reviewers and reduces the likelihood
that PRs will sit open for long periods of time.

![Contributing flow](https://github.com/tendermint/tendermint/blob/v0.33.6/docs/imgs/contributing.png?raw=true)

Each stage of the process is aimed at creating feedback cycles which align
contributors and maintainers in order to ensure that:
- Contributors don’t waste their time implementing/proposing features which won’t land in `master`
- Maintainers have the necessary context in order to support and review contributions

## Issues

We welcome bug reports, feature requests, and other contributions to our project.
To open an issue, please follow these guidelines:

1. **Search existing issues**: Before opening a new issue, please search existing issues to ensure that is not a duplicates.

2. **Provide a clear and descriptive title**: This helps others understand the nature of the issue at a glance.

3. **Provide detailed information**: In the issue description, clearly state the **purpose** of the issue
   include as much information as possible, such as:
    - Steps to reproduce the issue
    - Expected behavior
    - Actual behavior
    - The version of the operating system and the software you are using
    - Error messages or logs (if applicable)

4. **Use appropriate labels**: Assign relevant labels to indicate:
    - (WHY) The purpose or objective of this issue with Objective-level "O" labels
      like `O: security`, `O: new-feature`, etc.
    - (WHICH) The part of the system this issue relates to using:
      - External-level "E" labels if the issue fall outside the current scope of the system
        and is related to external dependencies or projects like `E: non-cosmos`, `E: no-std` etc.
      - or "Internal-level "I" labels for anything related to the current scope of the product
        like `I: logic`, `I: documentation`, `I: rpc`, etc.
    - (HOW) If any administrative considerations should be taken into account
      use Administrative-level "A" labels like `A: critical`, `A: help-wanted`, `A: risky`, etc.

This assist us prioritize and categorize your issue more effectively and help
others and reviewers understand the type and severity of the issue.
If the issue you worked on was tagged `A: low-priority`, we'll do our best to
review it in a timely manner, but please expect longer wait times for a review
in general. If a low priority issue is important to you, please leave a comment
explaining why, and we will reprioritize it!

## Pull Requests

If you have write access to the Hermes repo, you can directly branch off of `master`.
This makes it easier for project maintainers to directly make changes to your
branch should the need arise. Otherwise, check the [Forking](#forking) section for instructions.

Branch names should be prefixed with the author's name followed by a short description of the feature, eg. `name/feature-x`.

Pull requests are made against `master` and are squash-merged into master. Each
PR should:
- make reference to an issue outlining the context
- update any relevant documentation and include tests
- add a corresponding entry in the `.changelog` directory using `unclog`,
  see the [Changelog](#changelog) section for more details.

Additionally, in order to make PRs as easy to review as possible, each PR should:
- Be focused on implementing _*one*_ piece of logic from end-to-end. It must be 
very clear what the purpose of the PR is from looking at the PR's title, description, and/or linked issue(s). It should also be very clear what value the changes incorporated in the PR aim to deliver. A single PR that does multiple things, without a clear articulation of the problem it attempts to solve, will very likely be rejected. 
- Be small, ideally no more than 500 lines of code changes. While this is a guideline and not a hard rule, in general, larger changes should be structured as a series of PRs, each building off of the previous ones; these PRs should also be tracked in a tracking issue. 

   If a single PR absolutely has to be larger, it _must_ be structured such that it can be reviewed commit-by-commit, with each commit doing a single logical thing, accompanied with a good description of what it aims to achieve in the git commit message. Poorly structured PRs will likely be rejected on the grounds of being too much of a burden for the core maintainers to review; you will be asked to restructure the PR in accordance with the guidelines laid out here. 

   This does not necessarily apply to documentation-related changes or automatically generated code (e.g. generated from Protobuf definitions). But automatically generated code changes should occur within separate commits, so they are easily distinguishable from manual code changes.

In order to help facilitate the PR review process, tag *one* person as the
reviewer of the PR. If you are unsure of who to tag, your point of contact on
the Hermes team is always a natural candidate; they'll make sure that the PR gets
reviewed by whomever is most appropriate to review it. It also helps to notify
the person whom you tagged as reviewer through direct means, such as through
Slack or Discord, as it is easy for GitHub notifications to get lost or buried.

## Responsibilities of a PR Reviewer

If you're tagged as the reviewer of a PR, you are responsible for shepherding it
through to completion. This includes fixing issues with the PR and taking the
lead on decisions that need to be resolved in order to get the PR merged.

If you're tagged as a reviewer on a PR that affects a part of the code base that
you are unfamiliar with, you can hand it off to someone (with their
consent) who is more appropriate to shepherd the PR through to completion.

## Forking

If you do not have write access to the repository, your contribution should be
made through a fork on Github. Fork the repository, contribute to your fork
(either in the `master` branch of the fork or in a separate branch), and then
make a pull request back upstream.

When forking, add your fork's URL as a new git remote in your local copy of the
repo. For instance, to create a fork and work on a branch of it:
- Create the fork on GitHub, using the fork button.
- `cd` to the original clone of the repo on your machine
- `git remote rename origin upstream`
- `git remote add origin git@github.com:<location of fork>`

Now `origin` refers to your fork and `upstream` refers to the original version.
Now `git push -u origin master` to update the fork, and make pull requests
against the original repo.

To pull in updates from the origin repo, run `git fetch upstream` followed by
`git rebase upstream/master` (or whatever branch you're working in).

## Changelog

Every non-trivial PR must update the [CHANGELOG](CHANGELOG.md). This is
accomplished indirectly by adding entries to the `.changelog` folder in
[`unclog`](https://github.com/informalsystems/unclog) format using the `unclog` CLI tool.
`CHANGELOG.md` will be built by whomever is responsible for performing a release just
prior to release - this is to avoid changelog conflicts prior to releases.

### Install `unclog`

```bash
cargo install unclog
```

### Examples

Add a `.changelog` entry to signal that a bug was fixed, without mentioning any
component.

```bash
unclog add -i update-unclog-instructions -s bug -n 1634 -m "Update CONTRIBUTING.md for latest version of unclog" --editor vim
```

Add a .changelog entry for the `ibc-relayer-cli` crate (the component in the `relayer-cli`
directory) under the `FEATURES` section in CHANGELOG.md.

```bash
unclog add -c ibc-relayer-cli -s features --id a-new-feature --issue-no 1235 -m "msg about this new-feature" --editor vim
```

### Preview unreleased changes

```bash
unclog build -u
```

The Changelog is *not* a record of what Pull Requests were merged;
the commit history already shows that. The Changelog is a notice to users
about how their expectations of the software should be modified.
It is part of the UX of a release and is a *critical* user facing integration point.
The Changelog must be clean, inviting, and readable, with concise, meaningful entries.
Entries must be semantically meaningful to users. If a change takes multiple
Pull Requests to complete, it should likely have only a single entry in the
Changelog describing the net effect to the user. Instead of linking PRs directly, we
instead prefer to log issues, which tend to be higher-level, hence more relevant for users.

When writing Changelog entries, ensure they are targeting users of the software,
not fellow developers. Developers have much more context and care about more
things than users do. Changelogs are for users.

Changelog structure is modeled after
[Tendermint Core](https://github.com/tendermint/tendermint/blob/master/CHANGELOG.md)
and [Hashicorp Consul](http://github.com/hashicorp/consul/tree/master/CHANGELOG.md).
See those changelogs for examples.

We currently split changes for a given release between these four sections: Breaking
Changes, Features, Improvements, and Bug Fixes.

Entries in the changelog should initially be logged in the __Unreleased__ section, which
represents a "staging area" for accumulating all the changes throughout a
release (see [Pull Requests](#pull-requests) below). With each release,
the entries then move from this section into their permanent place under a
specific release number in Changelog.

Changelog entries should be formatted as follows:

```md
- [pkg] Some description about the change ([#xxx](https://github.com/informalsystems/hermes/issues/xxx)) (optional @contributor)
```

Here, `pkg` is the part of the code that changed (typically a
top-level crate, but could be `<crate>/<module>`), `xxx` is the issue number, and `contributor`
is the author/s of the change.

It's also acceptable for `xxx` to refer to the relevant pull request, but issue
numbers are preferred.
Note this means issues (or pull-requests) should be opened first so the changelog can then
be updated with the corresponding number.

Changelog entries should be ordered alphabetically according to the
`pkg`, and numerically according to their issue/PR number.

Changes with multiple classifications should be doubly included (eg. a bug fix
that is also a breaking change should be recorded under both).

Breaking changes are further subdivided according to the APIs/users they impact.
Any change that effects multiple APIs/users should be recorded multiply - for
instance, a change to some core protocol data structure might need to be
reflected both as breaking the core protocol but also breaking any APIs where core data structures are
exposed.

## Releases

Our release process is as follows:

1. Update the [changelog](#changelog) to reflect and summarize all changes in
   the release. This involves:
   1. Running `unclog build -u` and copy pasting the output at the top
      of the `CHANGELOG.md` file, making sure to update the header with
      the new version.
   1. Running `unclog release vX.Y.Z` to create a summary of all of the changes
      in this release.
   3. Committing the updated `CHANGELOG.md` file and `.changelog` directory to the repo.
2. Push this to a branch `release/vX.Y.Z` according to the version number of
   the anticipated release (e.g. `release/v0.18.0`) and open a **draft PR**.
3. Bump all relevant versions in the codebase to the new version and push these
   changes to the release PR. This includes:
   1. All `Cargo.toml` files (making sure dependencies' versions are updated
      too).
   2. All crates' `lib.rs` files documentation references' `html_root_url`
      parameters must point to the new version.
   3. Every reference to Hermes version in the [guide](./guide).
4. Run `cargo doc --all-features --open` locally to double-check that all the
   documentation compiles and is up-to-date and coherent. Fix any potential
   issues here and push them to the release PR.
5. Mark the PR as **Ready for Review** and incorporate feedback on the release.
6. Once approved, merge the PR.
7. Pull `master` and run the [`release.sh`](./scripts/release.sh) script.
   If any problem arises, submit a new PR, get it merged to `master` and try again.
   The reason for not releasing straight from the release branch, and therefore losing the
   ability to fix publishing-related problems as they arise, is that we would like the embedded
   metadata of the published crates, namely the Git commit at which the release was done,
   to match the Git commit on the `master` branch which will be tagged.
   [See this article][crates.io-security] for a more in-depth explanation.
   **Note:** This step requires the appropriate privileges to push crates to [crates.io].
8. Once all crates have been successfully released, create a signed tag and push it to
   GitHub: `git tag -s -a vX.Y.Z`. In the tag message, write the version and the link
   to the corresponding section of the changelog.
9. Once the tag is pushed, wait for the CI bot to create a GitHub release, and update
   the release description to `[📖 CHANGELOG](https://github.com/informalsystems/hermes/blob/master/CHANGELOG.md#vXYZ)`.
10. Wait an hour or so, and check that the CI job has uploaded the Hermes binaries to the release.
11. All done! 🎉

[crates.io]: https://crates.io
[crates.io-security]: https://codeandbitters.com/published-crate-analysis/
