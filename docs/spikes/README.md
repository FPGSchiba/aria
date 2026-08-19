# ARIA — Spike briefs

[Library index](../README.md) · [Decisions](../decisions/README.md) · [Open questions](../open-questions/README.md) · [Sprint 1](../sprints/sprint-01.md)

Research output for a specific spike story. A brief lives here from the moment work starts until
the spike closes — at which point its **conclusion** becomes a D-entry in
[`../decisions/`](../decisions/README.md) and the brief stays as the evidence behind it.

---

## What a brief is, and what it is not

A brief is **research and a recommendation**. It is not a decision, and it must not read like one.
The distinction is the same one that makes [`../open-questions/`](../open-questions/README.md)
work: a bucket-B item cannot be closed by anything written at a desk, only by a measurement.

So every brief here is explicit about which parts it can settle and which it cannot. Where a
brief proposes something, it says **proposal** and says who has to decide it.

Two habits worth keeping:

- **Name the conflict of interest** where one exists. The ARIA-40 brief compares LLM providers and
  was written by one of them; it says so at the top and pushes the answer onto the probe results.
- **Say when a source could not be verified.** A gap recorded as a gap is useful; a gap papered
  over with a plausible number is the failure this project is organised against.

---

## Current briefs

| Brief | Story | Status | Blocking |
|---|---|---|---|
| [ARIA-65 — Rust MCP SDK](ARIA-65-rust-mcp-sdk.md) | ARIA-65 | Recommendation ready (`rmcp` 3.1.3) | One PoC transcript |
| [ARIA-40 — Hosted LLM backend](ARIA-40-hosted-llm-backend.md) | ARIA-40 | Framework + probe harness ready | Running the probes with real API keys |
| [ARIA-43 — Knowledge classification](ARIA-43-knowledge-classification.md) | ARIA-43, ARIA-52 | Rule proposed; embedder still blocked | A decision on the rule; ARIA-27 for the embedder |
| [ARIA-79 — Cluster baseline](ARIA-79-cluster-baseline.md) | ARIA-79, ARIA-117 | Discovery script ready | A run against the real cluster |
| [ARIA-19 — Keycloak realm facts](ARIA-19-keycloak-facts.md) | ARIA-19 | Fill-in template ready | Realm access |
| [ARIA-109 — Which Ollama model](ARIA-109-ollama-model.md) | ARIA-109 | **Measured** — `qwen3:8b` recommended | A D-entry; one `--think` follow-up run |
| [ARIA-27 — GPU node baseline](ARIA-27-gpu-baseline.md) | ARIA-27, ARIA-79 | Hardware recorded; budget preliminary | VRAM at ARIA's real context length; reboot test |
| [ARIA-79 — Cluster findings 2026-08-19](ARIA-79-cluster-findings-2026-08-19.md) | ARIA-79, ARIA-117 | **9 findings; baseline run complete** | D6 reopening; GPU placement; capacity check |
| [Decision audit D1–D52](decision-audit-2026-08-19.md) | all | **1 decision superseded (D51→D52), 1 needs re-decision (D31)** | 4 discovery commands; a D31 re-decision |

## Companion scripts

| Script | Serves | Notes |
|---|---|---|
| [`../../scripts/aria-cluster-baseline.sh`](../../scripts/aria-cluster-baseline.sh) | ARIA-79, ARIA-117, ARIA-27 | Read-only. Deploys nothing |
| [`../../scripts/aria-llm-probe.py`](../../scripts/aria-llm-probe.py) | ARIA-40, **and ARIA-109** | Stdlib only. Same harness both sides, deliberately — ARIA-50's conformance suite is backend-agnostic |
| [`../../scripts/aria-ollama-diagnose.sh`](../../scripts/aria-ollama-diagnose.sh) | ARIA-27, ARIA-79 | Run in the container/host shell. Separates GPU-passthrough failure from model-thinking latency |
| [`../../scripts/aria-ollama-diagnose.ps1`](../../scripts/aria-ollama-diagnose.ps1) | ARIA-27 | Same, remotely over HTTP from Windows — no container shell needed |

---

## Closing a spike

1. Attach the evidence to the Jira story — a transcript, a probe result, a command output. The
   acceptance criteria on these stories ask for evidence specifically, not conclusions.
2. Write the decision as a D-entry with **rejected alternatives**.
3. Update the affected page (`02-stack.md`, a `services/` page, …).
4. Remove the item from its `open-questions/` bucket.
5. Leave the brief here. It is the audit trail for *why* the D-entry says what it says.
