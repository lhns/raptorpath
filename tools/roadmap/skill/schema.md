# Tech-tree schema

Three files per project: the config (`tech-tree.yaml`), the nodes (`nodes.yaml`, the one source of truth) and the main page (`roadmap.md` by default). `check.py` enforces everything below that is marked *checked*.

## Granularity: what is one node
One node is one thing that got, or will get, its own verdict. Apply the rules in this order; the first that decides wins.
1. **Own verdict.** It could have been merged, refuted or parked independently of its neighbours. A proposal with its revisions, trial runs, result and corrections is one node.
2. **Own evidence.** It has at least one record of its own: a merged branch or PR, a decision record, a results write-up, a design doc.
3. **Own edges.** Two items with the same prerequisites, dependants and outcome are one node; list the absorbed items in `folded`.
4. **Forward nodes:** one lane and one done criterion each (a passing check, a reviewed write-up, an owner call). A plan that needs two separate runs on a shared resource (a test environment, a benchmark machine) is two nodes.
5. **Always a node:**
   - each refuted or abandoned design (abandoned paths stay visible);
   - each owner decision that gates work;
   - each recorded defect finding (one that qualifies earlier results).
6. **Never a node:** a single commit, a bug fix inside a branch, updating a test's expected values, a tooling repair. These go in a node's `folded`.
7. **Sanity check:** the overview shows every month's important points; size its `overview` range to the history (months × picks per month). There is no per-track count: a track holds every node of its history, and its page lists them all.

## Node fields
```yaml
- id: api-rate-limit              # checked: kebab-case, unique, starts with "<track>-"
  title: "Per-key rate limiting"   # at most 6 plain words a newcomer can read (warnings: longer, or identifier-like)
  track: api                       # checked: a code from config tracks
  kind: feature                    # checked: a name from config kinds
  state: done                      # done | in-progress | next | blocked | parked | abandoned
  outcome: very-successful         # read off the quote: accepted, with a measured gain
  evidence: ["PR #212", "docs/adr/0007-rate-limits.md"]   # done needs at least one
  verdict_quote: "Merged; under the load test p99 410 ms -> 180 ms for well-behaved keys."  # verbatim, <= 300 chars
  headline: "p99 410 ms -> 180 ms" # one number with its unit, verbatim from the source; "" if none
  depends_on: [api-auth-keys]      # HARD prerequisites only; checked: exist, no cycle
  replaced_by: []                  # superseded/abandoned nodes: the node that replaced it
  lane: local                      # checked: a name from config lanes
  start: 2026-03-02                # YYYY-MM-DD or ""
  end: 2026-03-09                  # "" while open; checked: not before start
  folded: ["Retry-After header (2026-03-05, commit 3f9c2a1b)"]   # absorbed items, all shown on the track page
  overview: true                   # one of the ~screenful of nodes on the main overview
  # optional
  plain: "Each API key gets its own request budget, so one noisy client cannot slow the rest."
  notes: "Done when: ..."          # free text; forward nodes hold their done criterion here
  confirmed: 2026-03-12            # the date the owner confirmed this rating
  qualified_by: [data-defect-stale-cache]   # later defect nodes that qualify this done node's rating
  waiting_on: "staging deploy freeze"       # in-progress: built, waiting on something outside the work
  queue_order: 2                   # forward nodes in the config's queue lane: their order
  order_reason: "needs the seeded dataset from #1"
  diagram: false                   # leave out of the trees (tables and lists still show it)
```
The file is a bare list of nodes, or a mapping with the single key `nodes`.

## States and outcomes
`state` says where the work is; `outcome` says how it went. They are separate: a done node can be `mixed`, an abandoned one is `dead-end` or `superseded`.

Outcome scale. **An outcome other than `unrated` or `n/a` needs a `verdict_quote`** (checked), and is read off that quote, never inferred. A verdict is a sentence that says how the work went: a merge or PR description that states the result, a review approval, a release note, a results write-up, a decision record; owner statements made only in chat are paraphrased in `plain`, not quoted. **A merge alone is not a verdict:** `Merge branch 'x'` says the work landed, not how it went, so the node is done and `unrated`.

| outcome | the quoted verdict says |
| --- | --- |
| `very-successful` | it was accepted, with a measured gain (a number in `headline`) |
| `successful` | it was accepted and works as intended, no measured gain claimed |
| `mixed` | a gain with a stated cost, or a partial result |
| `bad` | it made things worse, or was merged and then reverted |
| `dead-end` | refuted or abandoned, nothing reused and nothing of it still in the product (check before rating; kept with caveats is `mixed`) |
| `superseded` | replaced by a named node (checked: `replaced_by` non-empty) |
| `unrated` | no recorded verdict yet |
| `n/a` | owner gates only (kinds with `owner_gate: true`): a decision is not rated |

Checked: `abandoned` needs `dead-end` or `superseded`; a kind with `rated: false` (a defect) stays `unrated`. Two aggregate warnings, one line each: done nodes still unrated, and ratings the owner has not confirmed (`confirmed` empty); the page says how many ratings are confirmed.

Readiness (checked): a node in state `next` must have every `depends_on` done, except prerequisites whose kind has `blocks: false` (a known defect does not block work on its diagnosis).

## Edges
- `depends_on`: hard prerequisites. Diagrams draw only the transitive reduction; tables keep every edge.
- `replaced_by`: old design to its replacement (dashed arrow).
- `qualified_by`: a done node to a later defect that weakens its rating (orange dotted arrow, and a warning mark in the plain-words table).

All diagrams run in one direction: time. A prerequisite that starts after its dependant is a warning.

## Folded items
Free text, one item each, all listed on the track page. A **labelled record** also appears under its node in *Done, by month*:
```
<title> (<YYYY-MM-DD>, commit <sha>[, commit <sha>...])
<title> (<YYYY-MM-DD>, session record)
```
An item carries its commits whenever one exists; `session record` is only for work with no committed artifact.
Backfilled history uses the labelled form, so every attached record is visible by date.

## Config (`tech-tree.yaml`)
Paths are relative to the config file. See `config.example.yaml` for an annotated one.

| key | meaning |
| --- | --- |
| `nodes` | path of nodes.yaml (default `docs/roadmap/nodes.yaml`) |
| `page` | path of the main page (default `docs/roadmap.md`); track pages go to `<page dir>/<page stem>/tracks/<code>.md`, the timeline to `<page dir>/<page stem>/timeline.md` |
| `tracks` | `{code: {name, scope}}`, in display order |
| `kinds` | `{name: {shape, rated, owner_gate, blocks}}`. `shape`: `box` (default), `slant`, `hexagon`, `flag`, `round`. `rated: false`: never carries an outcome, shown as open or fixed (defects). `owner_gate: true`: an owner decision; outcome `n/a`; once done, and when picked with `overview: true`, it is drawn in the overview only (never in the track trees), and open ones are listed under *Waiting on the owner*. `blocks: false`: an open node of this kind does not block its dependants' readiness. |
| `lanes` | list of lane names: who or what the work needs (a person, a machine, an environment) |
| `queue_lane` | optional: a lane whose forward nodes are worked one at a time; they carry a unique `queue_order` and appear in the *Queue* region |
| `overview` | optional `{min, max}`: warn when the overview count leaves this band; size it to the history (months × picks per month) |
| `layout` | optional `{overview_width, track_width, band_min}`, defaults 3, 3, 1: boxes per row in the overview and in track trees, and how many nodes a past-month overview band needs before the next month starts its own (1: one band per month) |
| `history` | optional, for `history_gap.py`: `main` (the main ref; default the first of origin/main, main, master), `weak_refs` (`{name: regex}` with one capture group, e.g. decision-record numbers or PR numbers; matched in commit messages and node text, counted as weak), `merge_subject` (a regex whose group 1 is the branch name in a merge commit subject, added to git's and GitHub's defaults) |

No track, kind or lane name is special to the scripts; behaviour comes from these attributes only.

## Generated regions
The main page holds hand-written text and generated regions between markers; the build rewrites only the regions and keeps every other byte:
```
<!-- BEGIN GENERATED: overview -->
...
<!-- END GENERATED: overview -->
```
Regions, each required exactly once: `legend`, `ratings`, `overview`, `plain-words`, `ready-now`, `running`, `built-waiting`, `waiting-on-owner`, `timeline`, `done-by-month`, `tracks`, and `queue` when `queue_lane` is set. Track pages and the timeline page are generated in full. A page holds at most 3 Mermaid diagrams (GitHub stops rendering some when a page holds more).

Derived regions, never hand-kept:
- **Ready now:** `next` nodes, by lane (owner gates excluded).
- **Running:** `in-progress` nodes without `waiting_on`.
- **Built, waiting:** `in-progress` nodes with `waiting_on`, grouped by it.
- **Waiting on the owner:** open owner-gate nodes.
- **Queue:** forward nodes in `queue_lane`, by `queue_order`.
- **Done, by month:** every done or abandoned node (with its tag) by the month it ended, with its plain sentence (else its headline) and its labelled folded records; a month in the span with nothing finished says so.

Warned: each past month with finished nodes but no overview node; titles that look like identifiers.

Checked: every recorded field of every node appears on a generated page (its track page shows the full row, details and folded items); the only exemptions are `overview` and `diagram`, which show by where the node is drawn. Stale regions or pages fail the check.
