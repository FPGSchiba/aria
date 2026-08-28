# ARIA — Decision Log · Retention & the audit record

**Decisions D55–D58** · taken 2026-08-20 (decision batch 1 — retention & what ARIA remembers)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md).

This batch closed **eleven open items across six issues**. Six of them were the same question —
*how long do we keep this?* — asked about five different stores. They are answered here as **one
retention schedule with a row per data class** (D56) rather than five separate windows, because
five windows decided in sequence become five inconsistent policies nobody can describe in a
sentence.

---

### D55 · An unrecognized-request record holds the raw utterance, under a short window, with a review path

**Decided.** The unrecognized-request log stores the **verbatim request text**, alongside the
`user_id` from the propagated context, a timestamp, and which LLM backend served it — i.e.
**ARIA-63's first acceptance criterion stands unchanged**. Its exposure is bounded by an explicit
retention window (**90 days**, see D56) rather than by redaction, and ARIA-108's query accessor is
the documented way it is reviewed. It is not a write-only store.

**Why.** The value of this log is the self-extension feedback loop D40 built the classifier for:
these rows are the gaps where a new MCP server would help, and one unknown is noise while the same
unknown eight times is a product requirement. That value is concentrated in the weeks after a
failure and it requires a **sentence**, not a counter — you cannot go and build the missing
capability from a timestamp and a retrieval score. Storing the text in Postgres is deliberately
**not** the same act as exporting it in a span: ARIA-74 keeps transcribed speech out of telemetry
because telemetry *leaves the cluster* (D41), and the same string is already stored verbatim by D36
in conversation history. The point of deciding both on the same day is that the second copy is a
marginal cost against an existing one rather than a new category of exposure.

**Accepted cost.** A durable, separately queryable index of exactly the requests ARIA failed to
understand — which skews toward unusual, emotional and private phrasing. Bounded, not eliminated,
by the 90-day window.

**Rejected.**

- *Metadata only* — timestamp, `user_id`, backend, retrieval top-score, tool-loop state. Nothing
  content-bearing would exist and retention would become almost uninteresting, consistent with what
  the collector is forced to do anyway. Rejected because you would see *that* ARIA failed and never
  *why*, which kills the loop the log exists for and would have meant shrinking ARIA-108 to a
  metrics artifact.
- *Redacted / normalised text* — reduces the obvious identifiers while keeping some diagnostic
  value. Rejected because entity-based redaction is unreliable on conversational speech and fails
  hardest on exactly the unusual phrasing this log is collecting; it adds a component and a failure
  mode, gives a false sense of safety, and still stores a sentence. **Verified 2026-08-20:** the
  off-the-shelf redaction that has genuinely improved sits inside *hosted* STT vendor APIs; nothing
  applicable to a self-hosted Rust pipeline, so this option also implied a new dependency ARIA does
  not have.
- *Raw text with no special window*, inheriting whatever the general policy is — least work, but it
  is the option that lets the log quietly become the longest-lived verbatim store in the system if
  conversation history is ever shortened. Under D56 history is kept indefinitely so that inversion
  does not arise today; the explicit window is what keeps it from arising later.
- *Hashing the text for deduplication without storing it* — a real technique, but a poor privacy
  control here: short utterances have low entropy and a hash of "unlock the front door" is
  recoverable by anyone who guesses the sentence.

**Consequence followed through.** The still-open ARIA-108 item *"whether unknowns feed back into
retrieval"* remains open and is **not** decided here — but it stays *possible* only because text
exists. Had this landed on metadata-only it would have become moot.

**Affects.** ARIA-108 (owner), ARIA-63 (payload criterion confirmed, not edited), ARIA-61.

---

### D56 · One retention schedule, with a row per data class

**Decided.** ARIA has **one** retention policy, expressed as the table below and cited from every
affected story rather than restated per story.

| Data class | Where | Retention | Mechanism |
|---|---|---|---|
| **Voice transcript** | `aria-speech` | **None.** Speech is stateless — no transcript persisted or logged at any level, and **partial hypotheses are never persisted at all**. One exception: an explicit, **off-by-default, time-boxed capture mode** for debugging, loud in logs when enabled, never on in a normal deploy | N/A |
| **Conversation history** | Postgres (Knowledge Core, D36) | **Indefinite**, as a deliberate choice | No pruning |
| **Unrecognized-request log** | Postgres (Knowledge Core) | **90 days**, on the record including its text (D55) | The pruning job below |
| **`consent_audit_log`** | Postgres (Knowledge Core) | **Indefinite**, as a deliberate choice | No pruning |
| **Telemetry** | Collector → destination | **Forward-only.** Nothing durable in-cluster beyond a bounded outage buffer. Retention at the destination is the destination's property — currently **30/45/60 days for traces depending on AppSignal plan** | Disk-backed sending queue; see below |

**Why, per row.**

**Voice transcript — nothing is kept.** At system level D36 already settled that conversation is
persisted; this row is the narrower, Speech-service question, and it is close to settled by existing
decisions. ARIA-74 already covers the *exported* case with a test that a marked string going in
produces no occurrence coming out. What it does not cover is a local file or a `tracing` line at
DEBUG that never leaves the pod — harmless in production and exactly what gets added during a bad
debugging afternoon and forgotten. Speech recognition in a pipeline architecture is conventionally
stateless: it decodes and forgets, and any durable copy lives one layer up where it is governed.
The debugging need is real, and the standard answer to it is a deliberate capture *mode* rather than
ambient logging. Partial hypotheses are singled out because they are wrong by construction and a
discarded partial is the worst possible thing to keep a durable copy of.

**Conversation history — kept indefinitely, and this is the accepted cost.** ARIA-115 exists
precisely to stop "keep forever" being inherited silently, so it is recorded here as a choice with
its cost named: **a permanent verbatim record of everything said in the house, including by circle
members who have no account (D4, D17), never logged in, and never agreed to anything.** The
reasoning for it: this is the store that makes "ARIA remembers what we talked about" true, and the
promotion path that a short window would have depended on (ARIA-43 / ARIA-76 — the mechanism that
turns a transcript into a durable fact) **is a story, not running code**. A short window decided
today would mean ARIA genuinely forgets until promotion lands. Note also the asymmetry that runs the
other way: you can always lengthen a window, you can never recover what a short one already deleted.

**Unrecognized-request log — 90 days, text and all.** The diagnostic value is concentrated in the
weeks after a failure, when the gap is still live and the pattern still forming; a year later either
the capability was built or nobody cared. 90 days catches a quarter's worth of pattern. It is
deliberately **not** the same answer as `consent_audit_log` despite ARIA-108's Jira text calling it
"the same gap": that is the same *gap*, not the same *answer* — the audit log is metadata about
actions and wants a long window, this is verbatim text about failures and wants a short one.

**`consent_audit_log` — kept indefinitely.** This is the one data class where the default instinct
should be *longer*, and it is unusually cheap: metadata only, bounded row size, no arguments or
results by ARIA-85's own criteria. Its value is entirely retrospective and appears exactly when
something has gone wrong. Decisively: under **D42** grants come into existence *conversationally*,
so this log is the **only place the origin of a grant is reconstructable** — a window shorter than
the lifetime of a grant would mean ARIA cannot answer where its own permissions came from. Whether
grants expire is a **separate open item, not decided here**; keeping the audit indefinitely sizes
for the pessimistic case and removes the coupling.

**Telemetry — forward only, with a buffer, stated destination-neutrally.** Two questions were
separated here. *(a) Is anything durable in-cluster?* For a collector this is really the question of
whether it has a disk-backed buffer, because a collector is a pipeline and not a store. ARIA takes a
**small disk-backed sending queue** — the `file_storage` extension, referenced from the exporter's
`sending_queue` — so a collector restart or a brief destination outage does not lose the spans in
flight, which are the ones you wanted during an incident. That buffer is measured in minutes; it is
not retention. *(b) How long do signals live at the destination?* That is a property of the
destination, not of ARIA, and it is **stated as an intent rather than a mechanism** because the
mechanism follows the **still-open D41 re-examination**.

**Why telemetry retention is the lowest-stakes row, and the caveat on that.** By ARIA-74's
default-deny design, exported telemetry carries no conversation content, no prompts, no tool
arguments and no retrieved knowledge — so this is an operational question rather than a privacy one.
Two things keep it from being trivial and are named here without being decided: **whether scrubbing
fails closed or open (ARIA-74) is still open**, so this row rests on an assumption that is not yet
guaranteed; and **whether `user_id` may appear in telemetry at all** is still open, and at ARIA's
scale a stable ID is very nearly identifying.

**Rejected.**

- *A fixed rolling window applied uniformly to every class* — one number, one job, trivially
  explainable. Rejected because it picks one number for purposes with very different half-lives:
  resuming a dropped session needs hours, reviewing a bad answer needs days, reconstructing where a
  permission came from needs years.
- *Tiered conversation history — verbatim short, derived facts long* — the standard shape for
  exactly this situation and the one most published schedules describe. Rejected **for now** because
  it is only as good as the promotion path, and promotion is not built. Most published schedules
  also come from multi-tenant services with legal exposure ARIA does not have; copying a number
  without its reasoning is not the same as having a policy.
- *Per-user retention setting, default short* — respects that a circle member may want zero
  retention, which is a real thing to want in a shared household. Not rejected on the merits;
  **deliberately deferred** for want of any UI to set it, in the same spirit as `consent_grants`'
  `granted_by` hook.
- *`consent_audit_log` pruned on the same window as conversation history* — one number for the whole
  system. Rejected on the merits: it conflates a forensic record with a content store.
- *A short window on the unknown log's text plus a long-lived derived record* (a count, a normalised
  form, a cluster) — would preserve seasonal and rare patterns past the text's life, which a plain
  window loses. Rejected because what the derived record actually *is* would be real design work in
  a story with no owner for it, and 90 days already covers a season.
- *Capping the unknown log by row count rather than age* — bounded storage regardless of volume and
  trivially implemented. Rejected because it makes the retention *statement* untellable: "we keep
  the last N" means the window silently shrinks the more ARIA is used, which is the wrong direction.
- *Persisting telemetry in-cluster as a real local store* — full control of retention, nothing
  leaves, and it removes the AppSignal-Rust-beta risk D41 itself flagged. Rejected **here** because
  it *is* the D41 re-examination, which deserves its own entry; deciding it as a side effect of a
  retention question would bypass that.
- *Matching telemetry retention to audit retention* so no trace ID ever dangles — rejected on cost
  structure: it keeps the highest-volume, least-valuable data longest, in a third party's system, to
  serve the lowest-volume data. This is the intuitive fix for D58's problem and it is the wrong one.

**One pruning job, one place for the policy.** Under this schedule **exactly one store is pruned**:
the unrecognized-request log. The policy therefore lives in one config surface and is applied by a
single CronJob in the umbrella chart (D15) calling a Knowledge Core operation — not as a retention
rule per table in whichever migration happened to add it, which is how a schedule drifts. Because
conversation history is not pruned, the job does **not** need to touch Qdrant, so the open D48
amendment question — *whether conversation history gets a derived Qdrant index* — is **untouched by
this decision** and stays open. **Verified 2026-08-20:** the umbrella chart does not exist in the
repository yet (no `deploy/` or `charts/` tree), so "the chart renders a CronJob" is new work, not an
existing capability.

**Retention and backups.** D32's off-site encrypted backups and the Postgres dumps in
`04-deployment.md` hold deleted rows for as long as the backup rotation says. Indefinite retention
on conversation history and the audit log means backup rotation **cannot** contradict them — there
is nothing deleted for a dump to preserve. The interaction is therefore narrow but real, and applies
to exactly one store: **a row pruned from the unrecognized-request log at 90 days survives in any
dump older than its deletion.** That is handed to **ARIA-96 / ARIA-119** rather than left as an
unexamined contradiction, and it is not solved here.

**One line on the legal framing, and only one.** Jann is in Switzerland, so the relevant instrument
is the revised FADP, not the GDPR. **Verified 2026-08-20 against the statute:** Art. 2 para 2 lit. a
excludes *"personal data being processed by a natural person exclusively for personal use."* Note
that this is **narrower than the GDPR's "purely personal or household activity"** wording the
framing is usually borrowed from — "exclusively for personal use" is a poorer fit for a system that
holds facts and recordings about several other people. This is **not briefed as a compliance
obligation**, because it is not one. What is worth taking from it is the principle behind both
regimes — collect what you need, keep it only as long as it is useful — and one practical asymmetry
that bears directly on the indefinite window chosen above: **facts and recordings about other people
in the circle are where a short retention is easiest to defend and a long one hardest**, because
those people did not choose this system.

**Affects.** ARIA-49 (transcript item closed), ARIA-115 (owner of the history row), ARIA-108,
ARIA-73, ARIA-85, ARIA-71. Hands the backup interaction to ARIA-96 / ARIA-119. Leans on, without
deciding: the **D41 re-examination**, **ARIA-74's scrubbing failure mode**, **whether `user_id` may
appear in telemetry**, and **whether consent grants expire**.

---

### D57 · Two audit rows per invocation — a durable attempt row before forwarding, an outcome row after

**Decided, as one design.** ARIA-73's and ARIA-85's two open items are the same question and are
answered together.

1. **Record shape.** Each tool invocation produces **two rows** correlated by an attempt id: an
   **attempt** row recording the gate decision (user, server, tool, allowed/denied, which gate,
   denial reason, timestamp, trace ID), and an **outcome** row recording what happened when the call
   was forwarded — including "the target server errored or was unreachable."
2. **Where the write sits.** The attempt row is appended **synchronously to durable local storage in
   `aria-mcp-registry` before the call is forwarded**. A background task drains the local outbox
   into the Knowledge Core. The outcome row is appended to the same outbox after the call returns
   and drains asynchronously.
3. **Failure behaviour on an *allowed* call.** Bounded retries, then the durable local queue, and
   the call **fails closed only if durability cannot be achieved anywhere**. The failure is loud.

**Why the two-row shape.** The single-row shapes are each incomplete, and the reason is structural
rather than a matter of taste. ARIA-85 requires an entry for calls that fail because the target
server errored — an outcome that only exists *after* forwarding. ARIA-73 makes the table append-only
at the database level, with a test proving `UPDATE` and `DELETE` fail. A row written before the call
therefore **cannot** be updated with the outcome afterwards. So a single row is either written
before forwarding (gates the call, cannot carry downstream failure) or after it (carries the
outcome, cannot gate anything). Two rows is the only shape that has both properties, and it is the
only shape under which "fail closed" means anything at all.

**Why the outbox, rather than a synchronous write to the Knowledge Core.** A plain synchronous
remote write would make ARIA's ability to *act* depend on the availability of a service that is not
otherwise on the critical path for the action — on a voice latency path, in a house. The outbox
converts the question "is the Knowledge Core reachable" into "is the local disk writable", which is
a far better question to make a tool call depend on.

**Why fail-closed-only-if-nowhere-durable, rather than plain fail-closed.** The useful reframing is
that the choice is not "block or drop" but **"how many places must be unavailable before we refuse
to act"**. Note also that half of this was already settled: ARIA-85 states that a failed audit write
for a **denied** call never causes the call to be allowed — the denial stands. Only the allowed
branch was open, and the asymmetry shows the project had already chosen a direction in the safer
case.

**Is this the same posture as D43? Deliberately argued, and the answer is no.** D43 fails closed
because a stale consent-cache entry means *ARIA does not know whether the user still permits this* —
**the permission itself is in doubt**. A failed audit write leaves no doubt at all about permission:
the gate said yes, on current data. Failing the call closed there protects the **record**, not the
user's control. Those are different goods, and the fail-closed instinct earned in the first does not
transfer automatically to the second. What survives is a weaker and more defensible version: refuse
to act only when the record cannot be made durable *anywhere*, which in practice means a genuinely
broken node.

**Rejected.**

- *One row after the call, in-memory buffered writer, drained async* — the industry default, with
  effectively zero latency cost and nothing new to deploy. Rejected because a pod kill loses
  whatever is in the channel, silently, and precisely during the incidents worth auditing; and
  because it makes fail-closed unavailable, so "every invocation ARIA made is in the log" would have
  to become "almost every invocation".
- *One row before forwarding, synchronous Knowledge Core write* — the strongest completeness claim
  and the cheapest honest version of it. Rejected because it puts a remote write on the voice path,
  turns a Postgres blip into a tool-call failure, and cannot carry the downstream outcome ARIA-85
  requires.
- *One row after the call via the durable outbox* — carries the outcome cleanly with near-zero
  remote dependency on the hot path. Rejected because it cannot gate: nothing prevents a call being
  forwarded with no record made.
- *Plain fail-closed on any audit-write failure* — extends D43's posture across the whole consent
  subsystem and makes the log evidence rather than an indication. Rejected: a lock that will not
  open because a write timed out is the failure that gets noticed at 2am, and per the D43 argument
  above the goods being protected are not the same.
- *Retry then proceed, gap surfaced loudly* — availability preserved with a bounded, visible,
  alertable gap. Rejected because with a durable outbox available the gap is avoidable, and this
  would have required softening the "every invocation is recorded" claim wherever it appears.
- *Fail closed only for high-risk tools* — proportionate; front-door locks block, volume changes do
  not. Rejected **today** because no risk-tier mechanism exists: tool risk tiering is a separate open
  item (ARIA-100), and choosing this would create a dependency on an undecided thing.

**Consequences followed through — four, and two of them are new work.**

1. **`aria-mcp-registry` is no longer stateless.** It needs durable local storage (a PVC or
   equivalent) plus drain, ordering and duplicate handling. `04-deployment.md` currently lists the
   Registry among the services that "can run on any node"; that is now false, and its scheduling and
   capacity claim must change. This lands on top of the already-open finding that the single
   schedulable node is tight (ARIA-79).
2. **ARIA-85's counting criterion is wrong as written.** "Entry count equals attempt count for a
   scripted mixed sequence" becomes **two** entries per attempt, correlated. That criterion is
   edited in the same pass as this decision.
3. **ARIA-73's column list grows** an attempt-correlation key and an entry-kind discriminator, and
   becomes load-bearing for a second reason under D58.
4. **Audit volume roughly doubles, against indefinite retention (D56).** ARIA-73's existing
   criterion — *"the expected write volume and the growth/retention position are documented"* — is
   now the thing standing between this design and unbounded growth. The retention position is
   satisfied by citing D56; **the volume estimate does not exist and is a measured tail**, below.

**Measured tail — deliberately not closed in conversation.** What the audit path actually costs is
unmeasured: neither service exists yet, and no spike owns it. Two numbers are needed and neither is
invented here — **(a)** the latency of the synchronous durable local append on the tool-call path,
and **(b)** the expected write volume and growth rate. Qualitative reasoning about the shape is safe
(a local durable append against a tool call that crosses the network to an MCP server and often to a
physical device), but a number is not, and no figure of the form "an insert is only a couple of
milliseconds" belongs in this entry. **(a)** is attached to **ARIA-85**, which owns the writer.
**(b)** is attached to **ARIA-73's** existing volume criterion.

**Affects.** ARIA-73 (schema, column list, growth criterion), ARIA-85 (owner of the writer; counting
criterion edited, failure-path criteria), ARIA-95 (the Registry failure-policy item's audit-write
clause is closed by this), ARIA-83 (via D58), `04-deployment.md`.

---

### D58 · The audit row stands alone; the trace ID is a best-effort hint expected to dangle

**Decided.** An audit record is **self-contained**. The fields that make it so are named and are
load-bearing: **user, server, tool, decision, which gate, denial reason, timestamp** — plus the
attempt-correlation key and entry kind added by D57. The **trace ID and span ID remain** on the row
as **opportunistic correlation**, documented as **best-effort and expected to resolve to nothing**
once the trace is gone. No trace summary is snapshotted into the row.

**Why.** The governing principle is old and unambiguous: if understanding an audit record requires
joining to a system with a different lifecycle, a different owner and a different retention, it is
not an audit record — it is a pointer. The standard implementation is deliberate denormalisation:
copy the human-meaningful facts in at write time, and treat the trace ID as a hint that is useful
while the trace lives and harmless when it does not.

**The dangle is now near-total, and that is the point.** Under D56 the audit log is kept
**indefinitely** while traces live **30–60 days** at the destination. The dangle window is therefore
not a narrow edge case — **essentially every audit row older than a month or two will carry a trace
ID that resolves to nothing.** That makes this decision more load-bearing than it looked when
ARIA-83 framed it, not less: the row has to carry its own meaning, because for almost all of its
life the trace behind it will not exist.

**Rejected.**

- *Snapshot a minimal trace summary* — duration, outcome, operation name copied into the audit row,
  preserving a little of the operational picture past the telemetry window. Rejected as coupling for
  its own sake: no question was identified that a duration would answer and the self-contained
  fields do not. It also carries a specific trap worth recording — **ARIA-74 lists span *names* as a
  leak path**, because a span name built from user input leaks as much as an attribute does, so
  "snapshot the span name" is not automatically safe and would need its own allow-list. Anything of
  this shape must not smuggle content back across ARIA-83's one-way boundary.
- *Accept the dangle and document only that* — zero work, and the trace ID is still useful during the
  window when someone is actively debugging. Rejected because without naming the self-contained
  field set, a future schema change can quietly drop one of the fields the row's independence rests
  on, and nobody would notice until an audit question could not be answered.
- *Extend telemetry retention to match audit retention* — the dangle would disappear entirely.
  Rejected under D56 on cost structure, and doubly so now that audit retention is indefinite: it is
  not even expressible.

**Affects.** ARIA-83 (owner — the correlation convention and its documentation criterion), ARIA-73
(its column list is now load-bearing for a second reason and should say so).

---
