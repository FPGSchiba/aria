# ARIA — Decision Log · Audio pipeline & turn-taking

**Decisions D24–D30** · taken 2026-08-12 (session 1)

One entry per decision: what was decided, why, what was rejected and why, and which stories it
affects. Part of [the decision index](README.md). Several entries carry an explicit **revisit
trigger** — those are not settled forever, but must not be reopened until the trigger fires.

---

### D24 · S1 — STT serves English only

**Established requirement, not a design choice.** ARIA is spoken to in English.

**Why it matters.** This is the input ARIA-30 could not conclude without. English-only removes the
multilingual model-size penalty entirely: small English-specific models become viable, which leaves
materially more VRAM headroom next to Ollama than any bilingual option would have. It also widens
TTS voice choice for ARIA-33.

**Rejected.** German+English (a larger multilingual model for code-switching), German only, and
German+English+Swiss German — the last of which would have been a research risk rather than a
configuration choice, since Swiss German is poorly served by every off-the-shelf model.

**Affects.** ARIA-30 (unblocked), ARIA-33 (unblocked), ARIA-27 (budget now less constrained),
ARIA-46, ARIA-49, ARIA-51.

---

### D25 · X3 — Opus on the client link, PCM internally

**Decided.** Client→Gateway carries Opus in 20 ms frames. The Gateway decodes to mono s16le PCM for
the Speech service. The exact model-facing sample rate is confirmed by ARIA-30 rather than asserted
here.

**Why.** Opus is what WebRTC uses, so a future browser or mobile client is trivial, and it survives
a phone on mobile data — which raw PCM would not, and external access is only deferred, not ruled
out. Internal PCM keeps Speech free of codec concerns.

**Rejected.**

- *Raw PCM end to end* — no codec anywhere, no encode/decode latency or quality loss, but ~256 kbps
  per stream rules out any non-LAN client.
- *Opus end to end* — saves a decode hop but couples Speech to the transport codec, so changing the
  client link later means changing Speech.

**Affects.** ARIA-26, ARIA-36, ARIA-37, ARIA-49.

---

### D26 · X4a — The Speech service owns VAD and endpointing

**Decided.** Speech owns voice-activity detection and utterance endpointing, emitting
utterance-boundary events upstream. The Gateway stays pure transport.

**Why.** Speech already owns audio processing and is the only component that knows what the chosen
STT model needs. Keeping the Gateway transport-only preserves §3's client-agnostic design and is
what allows a thin client to exist at all.

**Accepted cost.** Audio streams to the cluster during silence. Cheap on a LAN; revisit if external
access is designed.

**Rejected.**

- *Client-side VAD* — least bandwidth and server load, but every client type reimplements it and the
  Gateway would have to trust a client's claim about when a turn ended.
- *Gateway VAD* — saves GPU work, but makes the Gateway audio-aware and splits audio knowledge
  across two services, which is the exact ambiguity this decision exists to end.

**Affects.** ARIA-26, ARIA-36, ARIA-37, ARIA-39, ARIA-42, ARIA-49.

---

### D27 · X4b — One stream carries many turns; barge-in is supported

**Decided.** A `Converse` stream is a session carrying many turns, with Speech emitting utterance
boundaries as events inside it. Barge-in is supported: Speech keeps listening during synthesis and
signals speech-start, at which point the Gateway cancels the outbound TTS stream.

**Why.** Per-turn reconnection adds setup latency on a path where latency is the product, and being
unable to interrupt is precisely what makes voice assistants frustrating to use.

**Rejected.**

- *Many turns, no barge-in* — simpler state machine and no risk of ARIA's own voice triggering VAD,
  but not being able to interrupt is a real usability cost.
- *One stream per turn* — isolates failures and would have made G1 trivial, but adds connection
  setup to every turn.

**Newly revealed, and not small.** Barge-in requires that Speech not trigger on ARIA's own
synthesised voice. Acoustic echo cancellation — or half-duplex gating as a cheaper substitute — is
now required work that no story covers and no service owns. It plausibly belongs on the client,
which sits awkwardly with D26's thin-client reasoning.

**Affects.** ARIA-26, ARIA-36, ARIA-39, ARIA-49, ARIA-51.

---

### D28 · S6 — ARIA may change Ollama's server configuration

**Decided (Jann's call).** ARIA may reconfigure the shared Ollama instance — residency, concurrency,
model preloading — not merely set per-request parameters.

**Why.** It is the only way to actually guarantee a VRAM budget for Speech on a shared GPU. Without
it, Ollama holding a large model resident indefinitely could leave Speech unable to fit, with no
lever to pull.

**Rejected.**

- *Per-request `keep_alive` only* (my recommendation) — influences ARIA's own model residency
  without touching shared config, but provides no guarantee.
- *Treat Ollama as immutable* — cleanest boundary, but leaves Speech at the mercy of whatever else
  uses that GPU.

**Accepted cost, worth stating plainly.** This makes ARIA a co-owner of shared homelab
infrastructure, which narrows §6's "not standing up new infrastructure that duplicates what's
already running — extend the existing homelab instead" from *reuse without modification* to *reuse
with modification*. A change made for Speech's benefit affects everything else using that Ollama.
§4 and §6 need to say so rather than implying reuse-as-is.

**Affects.** ARIA-27, ARIA-46, ARIA-54, ARIA-93. Newly revealed: Ollama's configuration is now
ARIA-managed state that belongs in the Helm chart (D15) and needs recording somewhere.

---

### D29 · G1 — Bounded drain with an explicit session-ending control frame

**Decided.** On SIGTERM the Gateway stops accepting new streams, sends a "session ending, reconnect"
control frame on active ones, and closes after a short grace period (~30 s).

**Why.** A client that is told to reconnect behaves correctly; a client that sees a silent
mid-sentence drop during a routine deploy reads it as ARIA being broken.

**Rejected.**

- *Hard cut at the grace period* — nothing to build, but the in-flight utterance is lost with no
  signal.
- *Long grace period* — no client-side handling at all, but every deploy waits on the longest active
  conversation and a stuck stream blocks the rollout.

**Dependency.** Whether the reconnected session can resume context depends on X5 (batch 6).

**Affects.** ARIA-39, ARIA-36, ARIA-91.

---

### D30 · S5 — Speech owns TTS text normalisation, starting from the library's own handling

**Decided.** Normalisation lives in the Speech service as a preprocessing step before synthesis.
Start by relying on the chosen TTS library's built-in handling; add ARIA-specific rules only where
it demonstrably fails.

**Why.** Normalisation is a property of the voice/language pair ARIA-33 selects, not of the
assistant's reasoning. Building a normaliser before knowing what the chosen voice gets wrong is
work spent on a problem not yet observed.

**Rejected.**

- *Agent Core owns it* — it has the semantic context a downstream normaliser must guess at, but it
  would put speech concerns in the orchestrator and produce text that reads oddly on non-voice
  clients.
- *Rely entirely on the library* — zero work, but no place to fix it when it's wrong.

**Affects.** ARIA-51, ARIA-33.

---
