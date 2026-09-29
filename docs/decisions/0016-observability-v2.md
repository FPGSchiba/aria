# ARIA — Decision Log · Observability, second pass

**Decisions D82–D83** · taken 2026-09-23 (design conversation)

One entry per decision: what was decided, why, what was rejected and why, and what it affects.

D41 chose AppSignal's self-hosted collector as ARIA's single telemetry egress. That decision was
taken on 2026-08-12, **before** the ARIA-79 cluster baseline established what the homelab already
runs. The baseline found a Prometheus StatefulSet with node-exporter and kube-state-metrics in
`lens-metrics`, and a full Zabbix agent/proxy stack in `monitoring`, both running for 479 days
([decision audit](../spikes/decision-audit-2026-08-19.md)). D82 revisits the backend on those
facts. **The instrumentation does not change** — that is the point of having chosen a
vendor-neutral one.

---

### D82 · O1-v2 — Telemetry stays on the LAN: in-cluster collector, Prometheus, Jaeger, stdout

> ⚠ **Supersedes [D41](0007-observability-consent.md).** Instrumentation is unchanged; the backend
> and the egress claim are replaced.

**Decided.** Instrumentation stays **OpenTelemetry** — `tracing` + `tracing-opentelemetry` +
`opentelemetry-otlp` in every Rust service, exactly as D40/D41 specified. The backend changes:

| Signal | Destination |
|---|---|
| Metrics | the **existing Prometheus** in `lens-metrics`, via OTLP ingest |
| Traces | **Jaeger, deployed in-cluster** (new workload) |
| Logs | **stdout for now** — no export pipeline |

All three go through an **in-cluster OpenTelemetry collector** that ARIA owns. **AppSignal is
dropped.** **Telemetry no longer crosses the network boundary**, which removes it from §4's list of
things that leave.

**Why.** Two reasons, and the second is the stronger one.

1. **AppSignal would have been a third telemetry pipeline** standing next to Prometheus and Zabbix
   — both already running, both already administered by Jann, neither of which ARIA would have
   fed. Prometheus can ingest OTLP directly, so ARIA's metrics have a home that already exists.
2. **It shipped traces off-site.** §4's "local by default" was narrowed once already, to "no
   *personal* data leaves in plaintext", and telemetry was one of the three things named as
   crossing. Removing it narrows the exception list rather than widening it, and it makes the
   scrubbing question (D43/D44) a defence-in-depth measure rather than the only thing standing
   between a voice transcript and a third party.

Prometheus gives **no traces**, which is ARIA's actual need on a multi-service voice path — so a
local trace backend beside it is not optional, and Jaeger is the new deployment this decision
accepts.

**Accepted cost.**

- **A new workload to run and back up.** Jaeger is ARIA's to operate, on a cluster with exactly one
  schedulable node and tight capacity (ARIA-79, finding 6). AppSignal's appeal was that its
  storage, retention and UI were someone else's problem; that problem comes back in-house.
- **Retention becomes ARIA's decision, not a plan tier.** See the note below.
- **Two backends instead of one**, so there is no single pane correlating a metric spike with the
  trace that caused it. Prometheus and Jaeger are joined by hand, or by a Grafana that nobody has
  deployed.
- **Logs are unsolved, not solved.** "stdout for now" is honest about that: `kubectl logs` is the
  only way to read them, nothing aggregates across pods, and a pod restart loses history. The
  D41 rough edge (an immature Rust OTel logs bridge) is sidestepped rather than fixed.

**Invalidates a retention row.** [D56](0010-retention-audit.md)'s retention table states telemetry
retention as *"currently 30/45/60 days for traces depending on AppSignal plan"*. That row rests on a
vendor plan that no longer applies. **Jaeger's trace retention is now an open question** and is
recorded as one — it is not answered here. The interaction D56 already flagged still holds and
gets worse: a `consent_audit_log` row that outlives its trace now outlives it on ARIA's own
retention choice rather than a vendor's.

**Rejected.**

- *Keep AppSignal as decided in D41* — no new workload, hosted UI, retention and alerting included,
  and the free tier's headroom was already queued for measurement. Rejected because the off-site
  trace export is the part §4 most wants back, and because the third-pipeline argument only got
  stronger once the baseline showed what already runs. The beta status of AppSignal's Rust support,
  cited in §2 as a risk, is no longer a risk ARIA carries.
- *Metrics and traces both into Grafana's stack (Mimir/Tempo/Loki)* — one coherent stack with a
  single pane and a solved log story, which is the shape this would take if the cluster had room.
  Rejected on capacity: it is three new workloads plus Grafana on a one-node cluster, to replace a
  Prometheus that already works and that ARIA does not own. **Tempo remains the obvious substitute
  if Jaeger proves wrong** — vendor-neutral OTLP means that is a collector config change.
- *Prometheus only, no trace backend at all* — zero new workloads, and the thing Jann can already
  see. Rejected because a voice turn fans out across Gateway → Speech → Agent Core → Registry →
  an MCP server, and "where did the latency go" is unanswerable from metrics alone. Traces are the
  signal ARIA specifically needs and the one Prometheus cannot give.
- *Zabbix as the telemetry backend* — already running, already alerting, already administered.
  Rejected because Zabbix is the wrong shape: it monitors the cluster and hosts, not application
  traces. **This is not a rejection of Zabbix's role** — infrastructure alerting stays Zabbix's,
  and the decision audit's conclusion that "is `aria-speech` up, is the node healthy" is already
  solved stands unchanged.

**Affects.** ARIA-60, ARIA-66, ARIA-71, ARIA-74, ARIA-77 — all of which named AppSignal and now
need their backend references corrected. Also §2's observability row and §4's list of what crosses
the network boundary.

---

### D83 · O2 — Trace context propagates at four points, three of them by hand

**Decided.** A trace must survive a whole turn, and **four propagation points** are required. Only
one of them is automatic:

| Point | Mechanism | Automatic? |
|---|---|---|
| gRPC between ARIA services | a **`tonic` interceptor** injecting/extracting `traceparent` in metadata | **No** — written once, applied everywhere |
| Across the MCP boundary | a **`traceparent` header on the streamable-HTTP request** to the MCP server | **No** — and it must be in the **generated-server template** |
| From the client | **a turn starts on the device**, so the client creates the root span and sends its context to the Gateway | **No** |
| Around the LLM call | a span wrapping the hosted-API or Ollama call | broadly yes, within the process |

**The user-facing action timeline is built from the turn log ([D61](0012-clients-gateway-surface.md)),
never from traces.**

**Why.** Each of the three manual points is a place where a trace would otherwise end at the most
interesting moment.

- **The `tonic` interceptor** is unremarkable but has to exist; without it every service starts its
  own disconnected trace and the fan-out that D82 bought Jaeger for is invisible.
- **The MCP boundary is the one that would be forgotten.** MCP servers are written by anyone,
  including by ARIA herself via the Self-Extension server. If `traceparent` is not in the template
  `deploy_service` generates, every future server silently breaks the trace at exactly the hop
  where "what did the tool do" is asked. Putting it in the template is what makes this
  hold for servers nobody has written yet.
- **A turn starts on the device.** The user presses the button or speaks; network time, Opus
  encoding and stream setup all happen before the Gateway sees anything. A trace that starts at the
  Gateway cannot show why a turn felt slow.

The timeline rule is a **separation of audiences**. Traces are operational, sampled, and retained
on an operational schedule; the turn log is product data the user is entitled to see, retained
indefinitely (D56, D61). Building a user-facing feature on traces would either force trace
retention to match product retention or show the user a timeline with holes in it. **Traces may
carry a turn id for correlation; the timeline reads the log.**

**Accepted cost.**

- Client-side tracing means ARIA's trace boundary now includes code on devices ARIA does not
  operate, and a client can send a malformed or fabricated trace context. Nothing security-relevant
  rests on it — a trace id is not an identity — but it is another thing the Gateway takes from an
  untrusted source.
- Propagating `traceparent` **to a hosted LLM provider** discloses a correlation identifier to a
  third party. That question was already open before this decision and **stays open** — D83 does
  not settle it. The local Ollama fallback has no such objection.
- The generated-server template is now a correctness dependency of observability, which means a
  template change can silently degrade tracing for every server generated after it.

**Rejected.**

- *Rely on automatic instrumentation only* — nothing to write, and it works within a process.
  Rejected because it is wrong at exactly the three boundaries that matter; the result would look
  instrumented while showing five disconnected traces per turn.
- *Trace starts at the Gateway* — one less moving part, no trust placed in clients, and no client
  SDK to maintain across native and web. Rejected because it makes the pre-Gateway portion of a
  slow turn permanently invisible, which is where a voice product's latency complaints live.
- *Build the user-facing timeline from traces* — one source of truth, no second store, and the
  timeline would come free once tracing exists. Rejected because it couples product retention to
  operational retention and inherits sampling: a sampled-out turn would simply have no history,
  which is not an acceptable answer for a user asking what ARIA did on their behalf.

**Newly revealed.** Three things no story covers: who owns the client-side tracing library across
native and web clients; whether `traceparent` rides the outbound hosted-LLM call (pre-existing, now
sharpened); and whether a trace id is stored on the turn-log row to correlate the two audiences —
and if so, what happens when the trace behind it has already expired.

**Affects.** ARIA-60, ARIA-66, the generated-server template owned by the Self-Extension server,
and every service's instrumentation story.

---

## Related

- [Observability, consent & approval](0007-observability-consent.md) — D41, superseded here
- [Retention & audit](0010-retention-audit.md) — D56, whose telemetry row D82 invalidates
- [Clients, the turn log & the Gateway surface](0012-clients-gateway-surface.md) — D61, the turn log
- [Decision audit 2026-08-19](../spikes/decision-audit-2026-08-19.md) — the facts D82 rests on
