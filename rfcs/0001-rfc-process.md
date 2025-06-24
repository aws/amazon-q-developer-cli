- Name: RFC Process
- Start Date: 2025-03-19

# Summary

The "RFC" (request for comments) process is intended to provide a
consistent and controlled path for new features to enter the language
and standard libraries, so that all stakeholders can be confident about
the direction the language is evolving in.

# Motivation

This is a proposal for a more principled RFC process to make it
a more integral part of the overall development process, and one that is
followed consistently to introduce features to QCLI.

# Detailed design

Many changes, including bug fixes and documentation improvements can be
implemented and reviewed via the normal GitHub pull request workflow.

Some changes though are "substantial", and we ask that these be put
through a bit of a design process and produce a consensus among the
community and the maintainers.

## When you need to follow this process

You need to follow this process if you intend to make "substantial"
changes to QCLI. What constitutes a "substantial"
change is evolving based on community norms, but may include the following.

- Adding or removing features, including those that are feature-gated.
- Adding new crates or dependencies

Some changes do not require an RFC:

- Rephrasing, reorganizing, refactoring, or otherwise "changing shape
  does not change meaning".
- Additions that strictly improve objective, numerical quality
  criteria (warning removal, speedup, better platform coverage, more
  parallelism, trap more errors, etc.)
- Additions that are invisible to users of QCLI.

If you submit a pull request to implement a new feature without going
through the RFC process, it may be closed with a polite request to
submit an RFC first.

## What the process is

In short, to get a major feature added to QCLI, one must first get the
RFC merged into the RFC repo as a markdown file. At that point the RFC
is 'active' and may be implemented with the goal of eventual inclusion
into QCLI.

- Fork this repo.
- Copy `rfcs/0000-template.md` to `rfcs/0000-my-feature.md` (where
  'my-feature' is descriptive. don't assign an RFC number yet).
- Fill in the RFC
- Submit a pull request. The pull request is the time to get review of
  the design from the larger community.
- Build consensus and integrate feedback. RFCs that have broad support
  are much more likely to make progress than those that don't receive any
  comments.

Eventually, a maintainer will either accept the RFC by
merging the pull request, at which point the RFC is 'active', or
reject it by closing the pull request.

Whomever merges the RFC should do the following:

- Assign an id, using the PR number of the RFC pull request. (If the RFC
  has multiple pull requests associated with it, choose one PR number,
  preferably the minimal one.)
- Create a corresponding issue in this repo.
- Commit everything.

Once an RFC becomes active then authors may implement it and submit the
feature as a pull request to the repo. An active RFC is not a rubber
stamp, and in particular still does not mean the feature will ultimately
be merged; it does mean that in principle all the major stakeholders
have agreed to the feature and are amenable to merging it.

Modifications to active RFC's can be done in followup PR's. An RFC that
makes it through the entire process to implementation is considered
'complete'; an RFC that fails
after becoming active is 'inactive' and moves to the 'inactive' folder.

# Alternatives

Retain the current informal RFC process. The newly proposed RFC process is
designed to improve over the informal process in the following ways:

- Discourage unactionable or vague RFCs
- Ensure that all serious RFCs are considered equally
- Give confidence to those with a stake in Rust's development that they
  understand why new features are being merged

As an alternative, we could adopt an even stricter RFC process than the one proposed here. If desired, we should likely look to Python's [PEP] process for inspiration.
