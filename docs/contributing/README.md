# Contributing

This guide will help you get started. **Do not let this guide intimidate you**.
It should be considered a map to help you navigate the process.

## Quick start

If you are unsure where to begin, use the following guides:

- Want to report or triage a bug? Start with [Contributing in Issues](contributing-in-issues.md).
- Looking for something to work on? Check the open [`issues`](https://github.com/tokio-rs/tokio/issues)
(some might be in progress or still under discussion, so leave a comment before starting the work).
- Planning to submit a PR? Read [Pull Requests](pull-requests.md) for the full workflow and required checks.
- Want to understand what the labels on issues mean? See [Keeping track of issues and PRs](keeping-track-of-issues-and-prs.md).
- Interested in code review? See [Reviewing Pull Requests](reviewing-pull-requests.md).

## Step-by-Step Contribution Workflow

Once you've found an issue you'd like to work on, follow these steps before opening a Pull Request. This workflow provides a chronological overview of the contribution process and points to the relevant documentation where additional detail is available.

### 1. Fork and Clone the Repository

Fork the repository to your GitHub account and clone your fork locally.

If you've previously cloned the repository, ensure your local copy is up to date before creating a new branch.

```bash
git checkout master
git pull upstream master
```

---

### 2. Create a Feature Branch

Never commit directly to your local `master` branch. Instead, create a descriptive feature branch for each issue you work on.

```bash
git checkout -b fix/issue-####
```

Using a dedicated branch keeps your default branch clean and makes it easier to update your Pull Request during code review.

---

### 3. Implement Your Changes

Make the code or documentation changes needed to resolve the issue. Check your work with `git diff` to confirm the changes look correct before moving on.

If your changes introduce new functionality or modify existing behavior, consider whether additional tests or documentation should also be added.

---

### 4. Verify Your Changes

Before opening a Pull Request, run the project's verification steps. Refer to the [Pull Request Guidelines](pull-requests.md) for details on when each command should be used.

---

### 5. Commit and Push Your Changes

Commit your changes following the project's commit message guidelines.

```bash
git add .
git commit -m "docs: add contribution workflow (####)"
git push origin fix/issue-####
```

---

### 6. Open a Pull Request

Open a Pull Request from your feature branch to the main repository. See the [Pull Request Guidelines](pull-requests.md) for what to include and how the review process works.

## Table of Contents

- [Contributing in Issues](contributing-in-issues.md)
    - [Asking for General Help](contributing-in-issues.md#asking-for-general-help)
    - [Submitting a Bug Report](contributing-in-issues.md#submitting-a-bug-report)
    - [Triaging a Bug Report](contributing-in-issues.md#triaging-a-bug-report)
    - [Resolving a Bug Report](contributing-in-issues.md#resolving-a-bug-report)
- [Pull Requests](pull-requests.md)
    - [Cargo Commands](pull-requests.md#cargo-commands)
    - [Performing spellcheck on Tokio codebase](pull-requests.md#performing-spellcheck-on-tokio-codebase)
    - [Tests](pull-requests.md#tests)
        - [Integration tests](pull-requests.md#integration-tests)
        - [Fuzz tests](pull-requests.md#fuzz-tests)
        - [Documentation tests](pull-requests.md#documentation-tests)
    - [Benchmarks](pull-requests.md#benchmarks)
    - [Commits](pull-requests.md#commits)
        - [Commit message guidelines](pull-requests.md#commit-message-guidelines)
    - [Opening the Pull Request](pull-requests.md#opening-the-pull-request)
    - [Discuss and update](pull-requests.md#discuss-and-update)
    - [Commit Squashing](pull-requests.md#commit-squashing)
- [Reviewing Pull Requests](reviewing-pull-requests.md)
    - [Review a bit at a time](reviewing-pull-requests.md#review-a-bit-at-a-time)
    - [Be aware of the person behind the code](reviewing-pull-requests.md#be-aware-of-the-person-behind-the-code)
    - [Abandoned or Stalled Pull Requests](reviewing-pull-requests.md#abandoned-or-stalled-pull-requests)
- [How to specify crates dependencies versions](how-to-specify-crates-dependencies-versions.md)
- [Keeping track of issues and PRs](keeping-track-of-issues-and-prs.md)
    - [Area](keeping-track-of-issues-and-prs.md#area)
    - [Category](keeping-track-of-issues-and-prs.md#category)
    - [Calls for participation](keeping-track-of-issues-and-prs.md#calls-for-participation)
    - [Module](keeping-track-of-issues-and-prs.md#module)
    - [Topic](keeping-track-of-issues-and-prs.md#topic)
