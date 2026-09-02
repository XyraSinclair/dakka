# agent-deep-planning (fka dakka)

Deep planning through compositions of agents. **The discipline, not the
plumbing.**

> **Status: pure vibe-coded slop.** This is a pure experiment — a
> handful of scrappy prompts and a small runner to fire them, written by
> an agent, untested and unvalidated. It has completed a handful of
> end-to-end runs against real agent CLIs — the latest a 22-call
> deep-plan, unattended, four parallel planners, eight concurrent
> adjudications, stopping on a natural critique fixpoint at round 5 —
> and nothing beyond that. Read the code before trusting it with
> anything; expect sharp edges (stream-json capture keeps the model's
> narration blocks alongside the deliverable), and see SPEC.md's open
> questions for the known ones.

You have a hard problem and a laptop with agent CLIs on it (`claude`,
`codex`, `gemini`, a local model — anything with a prompt-in, text-out
CLI). dakka points a composition of them at your problem: planners that
never see each other's drafts, judged blind, arguing only where they
actually disagree, critiqued until a round finds nothing — and hands you
back a `PLAN.md` where every assumption carries the cheapest test that
would kill it.

```bash
dakka doctor                          # which harnesses you have
dakka pack --composition deep-plan --ask "..."   # see every payload before spending anything
dakka plan --ask "move the ingest fleet off the old scheduler without dropping a row"
dakka replan                          # after reality diverges: grade the plan's assumptions
```

## Why this is different

Every committee-of-models tool ships vague personas and provider
adapters. dakka ships neither:

- **No plumbing.** No API keys, no model roster, no context engine. It
  orchestrates the agent CLIs you already have — they bring auth, repo
  context, and fresh models. Adding a harness is five lines of
  `harnesses.toml`.
- **Measured words.** The arsenal (`arsenal/`) is prompt wordings with
  evidence attached — `yields.tsv` records what each operator actually
  did under blind judgment, with provenance. Thin evidence shows as
  thin. Operators earn their place or get pruned.
- **Discipline compiled to mechanism.** Blind judging is enforced (the
  tool shuffles, relabels, spawns a fresh judge). Incumbent quarantine
  is enforced (fresh-restart planners run in an empty temp dir — they
  *cannot* see your draft). Adjudication fires only on contested claims.
  Critique stops on a measured fixpoint, never a round count.

## Compositions

A composition is a small TOML file: linear stages, each = operator ×
harness × fan-out × judge rule. That's the whole grammar. The default
`deep-plan`: bind → diverge (N incumbent-blind planners) → blind judge →
route disagreement to trial → premortem → critique to fixpoint → compile
the questions only you can answer. Write your own; hand it to a friend —
a composition is how-to-think-hard, serialized.

## The flywheel

`dakka replan` grades the original plan's assumptions against what
actually happened; `dakka bench` fires an operator at your own repo.
Both feed the ledger. Operators and compositions accumulate track
records; the arsenal gets more accurate every time anyone fires it. You
are not using a tool; you are feeding one.

## Layout

    SPEC.md            the full spec
    arsenal/           operators.toml · yields.tsv · dispatch.toml
    compositions/      deep-plan · fresh · climb · replan
    harnesses.toml.example
    src/               the engine (~1.4k lines; a composition runner, nothing more)

## Setup

```bash
cp harnesses.toml.example harnesses.toml   # edit to what you have
dakka doctor
```
