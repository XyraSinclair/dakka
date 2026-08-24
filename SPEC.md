# dakka — spec draft v1 (2026-08-23)

## The promise

You have a hard problem and a laptop with three agent CLIs on it. Tonight
you can point twelve disciplined minds at that problem — minds that never
see each other's drafts, get judged blind, argue only where they actually
disagree, and hand you back one plan with every assumption named and a
test pinned to each one that would kill it.

That's dakka. The name is from the Orks, and we mean it: the answer to a
hard problem is more firepower. But Ork firepower sprays. Ours is
*sighted in* — every prompt in the arsenal has a measured kill-count, and
the arsenal gets more accurate every time anyone fires it.

**The discipline, not the plumbing.**

## Why this doesn't exist yet

Everyone who's tried to build this built the wrong half. The graveyard is
full of committee-of-models tools: five vague personas, provider
adapters, a context packer, a synthesis step that averages the committee
into porridge. Our own ancestor is in that graveyard —
`~/projects/archive/p1-moredakka`, 5,800 lines, beautiful README, zero
runs since April. It spent its engineering on plumbing around personas.

Two things changed:

1. **The plumbing became free.** `claude -p`, `codex exec`, `gemini` —
   people already own frontier harnesses with repo context, auth, and
   subscriptions built in. A tool that orchestrates *those* never rots
   when a provider ships a new model and never asks for an API key.
2. **We measured the discipline.** A year of hill-climb-parsimony
   experiments produced something nobody else ships: prompt wordings with
   yields. The champion deletion operator wins 16/17 under blind pairwise
   judgment. Self-advocacy framing: measured null — banned. "Unanimous"
   committee verdicts: 24 of 36 overturned under adjudication — so we
   adjudicate. Anchoring on the incumbent: real — so walkers never see
   it. Every one of those numbers is a design decision competitors are
   still making by vibes.

The scarce ingredient was never orchestration code. It's knowing which
words actually work when you fire them at a model, and having the
evidence behind each one.

## What a run feels like

You're staring at a migration you've put off for a month.

    $ dakka plan --ask "move the ingest fleet off the old scheduler without dropping a row"

`bind` states your objective back at you with the ambiguity you were
hiding from yourself. Then **six planners you never introduce to each
other** each design the migration from scratch — objective, constraints,
assumption ledger, nothing else. No incumbent to anchor on, no groupthink
possible by construction; different harnesses, different failure
profiles, six genuinely different plans. A blind judge — shuffled,
renamed, fresh context — picks a winner, and every loser's rejection
reason gets banked as a discovered constraint. The claims where the six
*disagreed* — only those — go to trial: prosecutor, advocate, arbiter.
Critique rounds hammer the survivor until a round finds nothing. Then it
compiles the residue down to the two questions only you can answer, and
everything else is already resolved in the plan.

Twenty minutes. You spent it making coffee. What's waiting is not a
report — it's a `PLAN.md` that fights back: every assumption carries the
cheapest experiment that would falsify it, every step carries "how we
know it worked," and when reality diverges next week, `dakka replan`
re-runs the composition against what actually happened and grades the
original's assumptions held/failed.

Those grades feed the ledger. Which means the thing that makes this
retardedly exciting: **the tool is a flywheel, not a product.** Every
replan, every bench run, every judged round makes the arsenal's numbers
harder. Operators that stop earning kills get pruned. Compositions
accumulate track records. You are not using a tool; you are feeding one.

## Compositions — spells you can hand to a friend

A composition is a small data file (`compositions/*.toml`): linear
stages, each stage = role wording × operator × harness × fan-out × judge
rule. That's the whole grammar — no conditionals, no loops; anything
fancier is a different composition. The engine is a ~300-line DAG runner
and stays that way.

This is the shareable unit. "Here's the composition I plan migrations
with" is a real sentence, a file, a repo star, a pull request. The best
ones will accumulate replan track records the way good benchmarks
accumulate citations. A composition is how-to-think-hard, serialized.

Default composition `deep-plan`:

1. **bind** — state the objective and its ambiguity explicitly (the
   inferred objective is the single largest error source in the business).
2. **diverge** — N fresh-restart planners; objective + constraints +
   assumption ledger ONLY, never a draft. Spend goes on divergence of
   plans, never personas debating one plan.
3. **judge** — blind: shuffled, renamed, fresh context. Losers' rejection
   reasons append to the assumption ledger as discovered constraints.
4. **route** — diff outputs into agreed vs contested; only contested
   claims go to prosecutor/advocate/arbiter. Adjudication's measured
   value (24/36 overturns) lives on the contested set; adjudicating
   consensus is spend without learning.
5. **fresh** — critique to fixpoint: rounds until a round changes nothing
   important. Premortem once, before round one.
6. **questions** — emit exactly what only the human can decide;
   everything else lands resolved.

## The plan is the artifact

dakka never emits a report that is not the plan. `PLAN.md`, fixed
domain-neutral sections, rewritten in place by every pass:

- objective, with its stated ambiguity
- options × objectives (a table, never a bare ranked verdict)
- chosen path
- **assumption ledger** — each assumption with the cheapest test that
  would falsify it. The highest-yield section in the file: most plans die
  of unstated assumptions, and pinning a test to each turns your plan
  into a research program.
- steps, each with its verification
- disagreement log — contested claims and how the trial came out
- questions for the human
- continuation handle — where to resume if interrupted

## The arsenal — three data files, no code

- **`operators.md`** — verbatim-fireable wordings. Seeded from the
  hill-climb-parsimony library plus: premortem; brenner slate (hypotheses
  with the third alternative and decisive tests); verification-per-step;
  reference class ("what happened last time someone tried this shape");
  wall detection ("is this plan patching around a problem the design
  should dissolve"); scope cut; adversarial user. The engine fires
  wordings verbatim, never paraphrases.
- **`yields.tsv`** — the measurements, with provenance columns
  (experiment id, target class, model, date). The measurements ARE the
  docs. Thin evidence shows as thin — that honesty is the moat, because
  nobody else can ship this file at all.
- **`dispatch.toml`** — circumstance→operator selection as readable,
  user-editable rules. The engine cannot accrete selection logic because
  selection lives in data.

Operators earn their place through `bench` and `replan` grades or get
pruned. Roles are not personas; they are operators with kill-counts.

## Discipline compiled to mechanism

Conscientiousness you request in a prompt evaporates under pressure.
Conscientiousness built into the executable cannot forget:

- **Blind judging is enforced, not requested** — the tool shuffles,
  renames A/B/C, spawns the judge with a fresh context.
- **Incumbent quarantine by construction** — fresh-restart walkers
  execute in a temp dir containing only the invariant list.
- **Adjudication over unanimity** — routed to contested claims only.
- **Disagreement preserved** in the artifact, never averaged away.
- **Stop on measured fixpoint** — a round that finds nothing. Never on
  predicted convergence, never on a round count.
- **Glass box before spend** — `dakka pack` / `--dry-run` print exactly
  which operators dispatch selected and the verbatim payload every walker
  will receive, costing nothing. You see the ammunition before you pay
  for the volley.

## Commands

    dakka plan [--composition deep-plan] --ask "..."   # the main verb
    dakka replan          # re-run vs executed state, grade assumptions
    dakka here            # infer objective from cwd, one bounded pass
    dakka review|patch --ask "..."
    dakka climb           # parsimony walk: reduce holding invariants
    dakka fresh           # critique to fixpoint
    dakka judge A B ...   # blind adjudication of existing candidates
    dakka pack|--dry-run  # show selection + payloads, spend nothing
    dakka bench           # measure an operator on your own repo
    dakka ledger          # yields per operator and per composition
    dakka doctor          # which harnesses are installed and authed

## Hard edges (what dakka refuses to be)

- **No model plumbing.** No provider SDKs, no API keys, no model roster,
  no context packer. Harness adapters are ~5 lines of `harnesses.toml`
  (command template, output capture, timeout). Adding a harness is
  config, not code.
- **No autonomy.** Emits plans and patch plans; your harness applies them.
- **No repo-context engine.** The harnesses already won that war.
- **1–2k lines, total.** Everything expensive is delegated. Any component
  that wants to grow past this is re-implementing a harness feature —
  kill it.

## Open questions

- **Seed:** fresh repo, with `~/projects/hill-climb-parsimony` `moves`
  data as the first arsenal payload (leaning), vs growing `moves` itself.
- **IP gate:** shipping publishes operator wordings + headline yields;
  raw experiment ledgers and exemplars stay home. Publication is Xyra's
  explicit call at every step.
- **Name:** dakka owns the joke and the lineage. Alternatives welcome but
  they'd have to beat it.
- **Route-stage fuzziness:** agreed-vs-contested claim diffing is judge-
  model-based in v0 and is the least-measured stage — first target for a
  /fresh pass and the first thing `bench` should learn to score.
- **From the moredakka archive:** mine the README philosophy section and
  the output-contract field list; none of the code.
