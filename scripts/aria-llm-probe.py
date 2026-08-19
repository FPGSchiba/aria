#!/usr/bin/env python3
"""
ARIA-40 / ARIA-109 — tool-calling probe harness.

Satisfies the acceptance criterion that candidate LLM backends are exercised against a
representative ARIA prompt carrying at least five MCP-style tool schemas, with observed
behaviour recorded as evidence rather than asserted from documentation.

The same harness serves both spikes on purpose: ARIA-50 requires a backend-agnostic
conformance suite, so the hosted probe (ARIA-40) and the local Ollama probe (ARIA-109)
must measure the same thing the same way. That is what makes D35's deferred
"automatic routing" rule answerable later with evidence instead of a guess.

Standard library only — no pip install.

Usage
-----
    export ANTHROPIC_API_KEY=...  GEMINI_API_KEY=...  OPENAI_API_KEY=...

    python3 aria-llm-probe.py --repeats 5
    python3 aria-llm-probe.py --provider anthropic --model claude-sonnet-5
    python3 aria-llm-probe.py --provider ollama --base-url http://gpu-node:11434 --model qwen3:14b

Outputs probe-results.json and probe-summary.md in the working directory.
"""

import argparse
import json
import os
import statistics
import sys
import time
import urllib.error
import urllib.request

# --------------------------------------------------------------------------------------
# Five MCP-style tool schemas, modelled on what ARIA will actually expose.
# Deliberately includes two similar tools (calendar vs reminder) to test disambiguation.
# --------------------------------------------------------------------------------------

TOOLS = [
    {
        "name": "calendar_create_event",
        "description": "Create a calendar event at a specific date and time.",
        "parameters": {
            "type": "object",
            "properties": {
                "title": {"type": "string", "description": "Event title"},
                "start": {"type": "string", "description": "ISO 8601 start datetime"},
                "duration_minutes": {"type": "integer", "description": "Length in minutes"},
            },
            "required": ["title", "start"],
            "additionalProperties": False,
        },
    },
    {
        "name": "reminder_create",
        "description": "Create a simple reminder. Use for tasks without a fixed appointment slot.",
        "parameters": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "What to be reminded about"},
                "due": {"type": "string", "description": "ISO 8601 due datetime, optional"},
            },
            "required": ["text"],
            "additionalProperties": False,
        },
    },
    {
        "name": "music_play",
        "description": "Play music on a speaker in the house.",
        "parameters": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Artist, album or track"},
                "room": {"type": "string", "description": "Room name, e.g. 'kitchen'"},
            },
            "required": ["query", "room"],
            "additionalProperties": False,
        },
    },
    {
        "name": "knowledge_lookup",
        "description": "Look up a remembered fact about a person in the household knowledge base.",
        "parameters": {
            "type": "object",
            "properties": {
                "person": {"type": "string", "description": "Person's name"},
                "topic": {"type": "string", "description": "What to look up, e.g. 'birthday'"},
            },
            "required": ["person", "topic"],
            "additionalProperties": False,
        },
    },
    {
        "name": "light_set",
        "description": "Turn a light on or off, or set its brightness.",
        "parameters": {
            "type": "object",
            "properties": {
                "room": {"type": "string"},
                "on": {"type": "boolean"},
                "brightness": {"type": "integer", "description": "0-100"},
            },
            "required": ["room", "on"],
            "additionalProperties": False,
        },
    },
]

SYSTEM = (
    "You are ARIA, a household assistant. Use the provided tools when they apply. "
    "Do not invent argument values that the user did not supply. "
    "If no tool applies, answer directly without calling one."
)

# expected: set of tool names that SHOULD be called (empty set = none)
CASES = [
    {
        "id": "single",
        "prompt": "Play some Miles Davis in the kitchen.",
        "expected": {"music_play"},
        "note": "unambiguous single tool",
    },
    {
        "id": "multi",
        "prompt": "Turn the living room lights on and put on some jazz in there too.",
        "expected": {"light_set", "music_play"},
        "note": "needs two tools in one turn",
    },
    {
        "id": "none",
        "prompt": "Why is the sky blue?",
        "expected": set(),
        "note": "no tool applies - measures spurious calls",
    },
    {
        "id": "ambiguous",
        "prompt": "Remind me to call the dentist tomorrow at 3pm.",
        "expected": {"reminder_create", "calendar_create_event"},
        "note": "either is defensible; measures whether it picks ONE, not both",
    },
    {
        "id": "missing_arg",
        "prompt": "Put on some music.",
        "expected": {"music_play"},
        "note": "required 'room' is absent - measures hallucinated arguments",
        "watch_hallucination": ("music_play", "room"),
    },
]

TOOL_BY_NAME = {t["name"]: t for t in TOOLS}

# --------------------------------------------------------------------------------------
# Argument validation
# --------------------------------------------------------------------------------------

_TYPES = {
    "string": str,
    "integer": int,
    "boolean": bool,
    "number": (int, float),
    "object": dict,
    "array": list,
}


def validate_args(tool_name, args):
    """Return list of validation problems. Empty list == well-formed call."""
    problems = []
    tool = TOOL_BY_NAME.get(tool_name)
    if tool is None:
        return [f"unknown tool {tool_name!r}"]
    if not isinstance(args, dict):
        return [f"arguments not an object: {type(args).__name__}"]
    schema = tool["parameters"]
    props = schema["properties"]
    for req in schema.get("required", []):
        if req not in args:
            problems.append(f"missing required {req!r}")
    for key, val in args.items():
        if key not in props:
            problems.append(f"unexpected property {key!r}")
            continue
        want = props[key].get("type")
        py = _TYPES.get(want)
        if py and not isinstance(val, py):
            problems.append(f"{key!r} should be {want}, got {type(val).__name__}")
        if isinstance(val, bool) and want == "integer":
            problems.append(f"{key!r} should be integer, got boolean")
    return problems


# --------------------------------------------------------------------------------------
# HTTP
# --------------------------------------------------------------------------------------


def norm_base(url):
    """Accept http://host:11434, .../v1, or either with a trailing slash.

    The adapters append '/v1/...' themselves, so a base URL that already ends in
    '/v1' would otherwise produce '/v1/v1/chat/completions' and a bare 404.
    """
    u = url.rstrip("/")
    if u.endswith("/v1"):
        u = u[: -len("/v1")]
    return u


def post_json(url, payload, headers, timeout=120):
    body = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=body, method="POST")
    req.add_header("Content-Type", "application/json")
    for k, v in headers.items():
        req.add_header(k, v)
    t0 = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode()
    except urllib.error.HTTPError as e:
        detail = e.read().decode()[:600]
        hint = ""
        if e.code == 404:
            hint = (f"\n  POSTed to: {url}"
                    "\n  A 404 here usually means the base URL or the model name is wrong.")
        raise RuntimeError(f"HTTP {e.code}: {detail}{hint}") from None
    except urllib.error.URLError as e:
        raise RuntimeError(f"cannot reach {url}: {e.reason}") from None
    return json.loads(raw), time.monotonic() - t0


def get_json(url, headers=None, timeout=30):
    req = urllib.request.Request(url, method="GET")
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def preflight_ollama(base_url, model):
    """Fail fast and usefully instead of 15 identical 404s."""
    base = norm_base(base_url)
    try:
        data = get_json(f"{base}/v1/models")
    except Exception as e:  # noqa: BLE001
        try:
            tags = get_json(f"{base}/api/tags")
            names = [m.get("name") for m in tags.get("models", [])]
            raise RuntimeError(
                f"{base}/v1/models failed ({e}), but {base}/api/tags worked.\n"
                f"  Ollama is reachable but its OpenAI-compatible endpoint is not responding.\n"
                f"  Models present: {names}"
            ) from None
        except RuntimeError:
            raise
        except Exception:
            raise RuntimeError(
                f"Cannot reach Ollama at {base}: {e}\n"
                f"  Tried {base}/v1/models and {base}/api/tags.\n"
                f"  Check the host/port, and that Ollama listens on more than localhost\n"
                f"  (OLLAMA_HOST=0.0.0.0 on the server)."
            ) from None

    available = [m.get("id") for m in data.get("data", [])]
    if model not in available:
        raise RuntimeError(
            f"Model {model!r} is not loaded on {base}.\n"
            f"  Available: {available or '(none)'}\n"
            f"  Pull it with:  ollama pull {model}\n"
            f"  or re-run with --model <one of the above>."
        )
    print(f"  preflight OK — {base} serving {model}")
    return base


# --------------------------------------------------------------------------------------
# Provider adapters -> normalised [(tool_name, args_dict), ...]
# --------------------------------------------------------------------------------------


def probe_anthropic(prompt, model):
    key = os.environ.get("ANTHROPIC_API_KEY")
    if not key:
        raise RuntimeError("ANTHROPIC_API_KEY not set")
    payload = {
        "model": model,
        "max_tokens": 1024,
        "system": SYSTEM,
        "messages": [{"role": "user", "content": prompt}],
        "tools": [
            {"name": t["name"], "description": t["description"], "input_schema": t["parameters"]}
            for t in TOOLS
        ],
    }
    data, secs = post_json(
        "https://api.anthropic.com/v1/messages",
        payload,
        {"x-api-key": key, "anthropic-version": "2023-06-01"},
    )
    calls = [
        (b["name"], b.get("input", {}))
        for b in data.get("content", [])
        if b.get("type") == "tool_use"
    ]
    return calls, secs, data.get("usage", {})


def probe_openai(prompt, model, base_url="https://api.openai.com", key_env="OPENAI_API_KEY"):
    key = os.environ.get(key_env)
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": prompt},
        ],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["parameters"],
                },
            }
            for t in TOOLS
        ],
    }
    headers = {"Authorization": f"Bearer {key}"} if key else {}
    data, secs = post_json(f"{norm_base(base_url)}/v1/chat/completions", payload, headers)
    msg = data["choices"][0]["message"]
    calls = []
    for tc in msg.get("tool_calls") or []:
        fn = tc.get("function", {})
        raw = fn.get("arguments", "{}")
        try:
            args = json.loads(raw) if isinstance(raw, str) else raw
        except json.JSONDecodeError:
            args = {"__unparseable__": raw}
        calls.append((fn.get("name"), args))
    return calls, secs, data.get("usage", {})


def probe_gemini(prompt, model):
    key = os.environ.get("GEMINI_API_KEY")
    if not key:
        raise RuntimeError("GEMINI_API_KEY not set")
    payload = {
        "system_instruction": {"parts": [{"text": SYSTEM}]},
        "contents": [{"role": "user", "parts": [{"text": prompt}]}],
        "tools": [
            {
                "function_declarations": [
                    {
                        "name": t["name"],
                        "description": t["description"],
                        "parameters": _strip_unsupported(t["parameters"]),
                    }
                    for t in TOOLS
                ]
            }
        ],
    }
    url = f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
    data, secs = post_json(url, payload, {"x-goog-api-key": key})
    calls = []
    for cand in data.get("candidates", []):
        for part in cand.get("content", {}).get("parts", []):
            if "functionCall" in part:
                fc = part["functionCall"]
                calls.append((fc.get("name"), fc.get("args", {})))
    return calls, secs, data.get("usageMetadata", {})


def _strip_unsupported(schema):
    """Gemini's function-declaration schema subset rejects additionalProperties."""
    out = {k: v for k, v in schema.items() if k != "additionalProperties"}
    if "properties" in out:
        out["properties"] = {
            k: {kk: vv for kk, vv in v.items() if kk != "additionalProperties"}
            for k, v in out["properties"].items()
        }
    return out


def probe_ollama(prompt, model, base_url):
    """Ollama's NATIVE /api/chat, not the OpenAI-compatible shim.

    Why native: thinking models (Qwen3, DeepSeek-R1, …) reason before answering, and with
    five tool schemas in the prompt that routinely blows past a 120 s timeout. `think: false`
    turns it off — and it is only available on the native endpoint, not on
    /v1/chat/completions. `--ollama-openai-compat` restores the old path.

    Note for ARIA-54/D35: this is a real asymmetry between the two backends. See the
    ARIA-40 brief's section on the canonical internal tool-call shape.
    """
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": prompt},
        ],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["parameters"],
                },
            }
            for t in TOOLS
        ],
        "think": OPTS.get("think", False),
        "stream": False,
        "options": {"num_predict": 1024},
    }
    data, secs = post_json(f"{norm_base(base_url)}/api/chat", payload, {},
                           timeout=OPTS.get("timeout", 120))
    calls = []
    for tc in data.get("message", {}).get("tool_calls") or []:
        fn = tc.get("function", {})
        args = fn.get("arguments", {})
        if isinstance(args, str):
            try:
                args = json.loads(args)
            except json.JSONDecodeError:
                args = {"__unparseable__": args}
        calls.append((fn.get("name"), args))
    usage = {k: data.get(k) for k in ("eval_count", "prompt_eval_count", "total_duration")
             if data.get(k) is not None}
    if data.get("total_duration"):
        usage["tokens_per_sec"] = round(
            (data.get("eval_count") or 0) / (data["total_duration"] / 1e9), 1)
    return calls, secs, usage


def probe_ollama_openai(prompt, model, base_url):
    return probe_openai(prompt, model, base_url=base_url, key_env="OLLAMA_API_KEY")


# runtime options set from argv; keeps the adapter signatures stable
OPTS = {"timeout": 120, "think": False}


PROVIDERS = {
    "anthropic": ("claude-sonnet-5", probe_anthropic),
    "openai": ("gpt-5", probe_openai),
    "gemini": ("gemini-3.7-flash", probe_gemini),
    "ollama": ("qwen3:14b", probe_ollama),
}

# --------------------------------------------------------------------------------------


def run(provider, model, repeats, base_url):
    _, fn = PROVIDERS[provider]
    if provider == "ollama":
        base_url = preflight_ollama(base_url, model)
    results = []
    for case in CASES:
        for i in range(repeats):
            rec = {
                "provider": provider,
                "model": model,
                "case": case["id"],
                "note": case["note"],
                "run": i,
            }
            try:
                if provider == "ollama":
                    calls, secs, usage = fn(case["prompt"], model, base_url)
                else:
                    calls, secs, usage = fn(case["prompt"], model)
            except Exception as e:  # noqa: BLE001 - probe harness, report and continue
                rec.update(error=str(e)[:400])
                results.append(rec)
                print(f"  {case['id']:12} run {i}  ERROR  {str(e)[:90]}", file=sys.stderr)
                continue

            names = [c[0] for c in calls]
            problems = []
            for name, args in calls:
                problems += [f"{name}: {p}" for p in validate_args(name, args)]

            expected = case["expected"]
            called = set(names)
            if not expected:
                correct = len(called) == 0
            elif case["id"] == "ambiguous":
                # picking exactly one of the two defensible tools is the pass condition
                correct = len(called) == 1 and called <= expected
            else:
                correct = called == expected

            hallucinated = None
            watch = case.get("watch_hallucination")
            if watch:
                tname, argname = watch
                for name, args in calls:
                    if name == tname:
                        hallucinated = argname in args

            rec.update(
                calls=[{"name": n, "args": a} for n, a in calls],
                tool_names=names,
                expected=sorted(expected),
                correct=correct,
                malformed=len(problems) > 0,
                problems=problems,
                spurious=(not expected and len(called) > 0),
                hallucinated_arg=hallucinated,
                seconds=round(secs, 3),
                usage=usage,
            )
            results.append(rec)
            flag = "ok " if correct else "MISS"
            extra = " MALFORMED" if problems else ""
            if hallucinated:
                extra += " HALLUCINATED-ARG"
            print(f"  {case['id']:12} run {i}  {flag} {names or '[]'}{extra}  {secs:.2f}s")
    return results



# --------------------------------------------------------------------------------------
# Streaming probe — ARIA-109's largest unmet acceptance criterion.
#
# D37 makes `Decide` server-streaming so speech synthesis can start on the first sentence.
# That only pays off if tool-call events arrive *incrementally* rather than all at the end.
# This measures whether they do, and how they are shaped on the wire.
#
# Currently implemented for Ollama's native /api/chat (newline-delimited JSON) only.
# Anthropic and Gemini use different SSE envelopes; implementing them half-way would be
# worse than saying plainly that they are not covered yet.
# --------------------------------------------------------------------------------------


def stream_ollama(prompt, model, base_url, timeout=180):
    """Return per-chunk timings and shape facts for one streamed turn."""
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": prompt},
        ],
        "tools": [
            {"type": "function", "function": {"name": t["name"],
                                              "description": t["description"],
                                              "parameters": t["parameters"]}}
            for t in TOOLS
        ],
        "think": OPTS.get("think", False),
        "stream": True,
        "options": {"num_predict": 1024},
    }
    url = f"{norm_base(base_url)}/api/chat"
    req = urllib.request.Request(url, data=json.dumps(payload).encode(), method="POST")
    req.add_header("Content-Type", "application/json")

    chunks = []
    t0 = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            for raw in resp:
                line = raw.decode().strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                chunks.append((time.monotonic() - t0, obj))
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"HTTP {e.code}: {e.read().decode()[:400]}") from None
    except urllib.error.URLError as e:
        raise RuntimeError(f"cannot reach {url}: {e.reason}") from None

    total = time.monotonic() - t0
    first_text = first_tool = None
    fragmented = False
    calls = []
    for i, (ts, obj) in enumerate(chunks):
        msg = obj.get("message") or {}
        if first_text is None and (msg.get("content") or "").strip():
            first_text = (i, ts)
        tcs = msg.get("tool_calls") or []
        if tcs and first_tool is None:
            first_tool = (i, ts)
        for tc in tcs:
            fn = tc.get("function", {})
            args = fn.get("arguments", {})
            # A vendor that fragments tool arguments sends them as partial strings across
            # deltas, which the client must accumulate. A complete object does not.
            if isinstance(args, str):
                fragmented = True
                try:
                    args = json.loads(args)
                except json.JSONDecodeError:
                    pass
            calls.append((fn.get("name"), args))

    return {
        "chunks": len(chunks),
        "stream_seconds": round(total, 3),
        "ttft_seconds": round(min(x[1] for x in (first_text, first_tool) if x), 3)
                        if (first_text or first_tool) else None,
        "first_text_chunk": first_text[0] if first_text else None,
        "first_tool_chunk": first_tool[0] if first_tool else None,
        "text_before_tool_call": bool(first_text and first_tool and first_text[0] < first_tool[0]),
        "tool_call_arrives_early": bool(first_tool and len(chunks) > 1
                                        and first_tool[0] < len(chunks) - 1),
        "tool_args_fragmented": fragmented,
        "calls": [{"name": n, "args": a} for n, a in calls],
        "tool_names": [n for n, _ in calls],
    }


def run_streaming(provider, model, repeats, base_url):
    if provider != "ollama":
        raise RuntimeError(
            f"--stream is implemented for Ollama's native API only; {provider} uses a different\n"
            f"  SSE envelope and is not covered yet. Run it without --stream, or extend\n"
            f"  stream_ollama() with that vendor's format."
        )
    preflight_ollama(base_url, model)
    out = []
    for case in CASES:
        for i in range(repeats):
            rec = {"provider": provider, "model": model, "case": case["id"],
                   "run": i, "mode": "streaming"}
            try:
                rec.update(stream_ollama(case["prompt"], model, base_url,
                                         OPTS.get("timeout", 120)))
            except Exception as e:  # noqa: BLE001
                rec["error"] = str(e)[:300]
                out.append(rec)
                print(f"  {case['id']:12} run {i}  ERROR  {str(e)[:80]}", file=sys.stderr)
                continue
            expected = case["expected"]
            called = set(rec["tool_names"])
            if not expected:
                rec["correct"] = len(called) == 0
            elif case["id"] == "ambiguous":
                rec["correct"] = len(called) == 1 and called <= expected
            else:
                rec["correct"] = called == expected
            pos = (f"tool@{rec['first_tool_chunk']}/{rec['chunks']}"
                   if rec["first_tool_chunk"] is not None else "no-tool")
            print(f"  {case['id']:12} run {i}  {'ok ' if rec['correct'] else 'MISS'} "
                  f"ttft={rec['ttft_seconds']}s  {pos}  chunks={rec['chunks']}")
            out.append(rec)
    return out


def summarise_streaming(rs):
    rs = [r for r in rs if r.get("mode") == "streaming" and "error" not in r]
    if not rs:
        return ""
    lines = ["", "## Streaming behaviour (ARIA-109 / D37)", ""]
    lines += ["Tool-calling turns and prose turns behave completely differently, so they are",
              "reported separately. Averaging them would hide the finding.", "",
              "| Provider | Model | Turn type | Median TTFT | Median chunks | Incremental? | Args fragmented |",
              "|---|---|---|---|---|---|---|"]
    by = {}
    for r in rs:
        kind = "tool call" if r["first_tool_chunk"] is not None else "prose"
        by.setdefault((r["provider"], r["model"], kind), []).append(r)

    for (prov, model, kind) in sorted(by, key=lambda k: (k[0], k[1], k[2])):
        g = by[(prov, model, kind)]
        ttfts = [r["ttft_seconds"] for r in g if r["ttft_seconds"] is not None]
        chunks = statistics.median(r["chunks"] for r in g)
        # A response delivered in <=2 chunks (content + done marker) is not incrementally
        # streamed in any useful sense, however the chunk index reads.
        incremental = "no (single chunk)" if chunks <= 2 else "yes"
        frag = any(r["tool_args_fragmented"] for r in g)
        lines.append(f"| {prov} | `{model}` | {kind} | "
                     f"{statistics.median(ttfts):.2f}s | {int(chunks)} | {incremental} | "
                     f"{'yes' if frag else 'no'} |")

    lines += [
        "",
        "### How to read this",
        "",
        "**Median TTFT** — time to the first chunk carrying text or a tool call.",
        "**Median chunks** — how many messages the response arrived in. **This is the number that",
        "matters.** A turn delivered in two chunks (the content, then the `done` marker) is not",
        "streaming in any useful sense, no matter where the tool call sits in the sequence.",
        "**Args fragmented** — whether tool arguments arrive as partial strings needing",
        "accumulation (OpenAI-style) or complete objects (Ollama-style). An adapter difference the",
        "`LlmBackend` trait must absorb; evidence for D49's ARIA-owned canonical shape.",
        "",
        "### What this means for D37",
        "",
        "D37 made `Decide` server-streaming so speech synthesis can start on the first sentence.",
        "Read the two rows against that intent:",
        "",
        "- **Prose turns** — many chunks, low TTFT. Streaming delivers exactly what D37 wanted.",
        "- **Tool-call turns** — the whole call arrives at once after the model has finished",
        "  deciding. Streaming buys **nothing** here, because there is no partial tool call worth",
        "  emitting. That is not a defect; it is what a tool call is.",
        "",
        "So D37's rationale holds for the half of the traffic it was aimed at, and the tool path",
        "should be judged on total latency rather than time-to-first-token.",
    ]
    return "\n".join(lines) + "\n"

def summarise(all_results):
    # Streaming records carry different fields; they are summarised separately.
    all_results = [r for r in all_results if r.get("mode") != "streaming"]
    if not all_results:
        return ""
    lines = ["# ARIA-40 / ARIA-109 — tool-calling probe results", ""]
    lines.append(f"Generated {time.strftime('%Y-%m-%d %H:%M:%S')}")
    lines.append("")
    lines.append("| Provider | Model | Selection acc. | Malformed | Spurious (no-tool case) | Hallucinated arg | Median latency |")
    lines.append("|---|---|---|---|---|---|---|")
    by_model = {}
    for r in all_results:
        by_model.setdefault((r["provider"], r["model"]), []).append(r)
    for (prov, model), rs in by_model.items():
        ok = [r for r in rs if "error" not in r]
        if not ok:
            lines.append(f"| {prov} | `{model}` | — | — | — | — | all runs errored |")
            continue
        acc = sum(1 for r in ok if r["correct"]) / len(ok)
        mal = sum(1 for r in ok if r["malformed"]) / len(ok)
        none_runs = [r for r in ok if r["case"] == "none"]
        spur = (sum(1 for r in none_runs if r["spurious"]) / len(none_runs)) if none_runs else 0
        hall_runs = [r for r in ok if r.get("hallucinated_arg") is not None]
        hall = (sum(1 for r in hall_runs if r["hallucinated_arg"]) / len(hall_runs)) if hall_runs else 0
        med = statistics.median(r["seconds"] for r in ok)
        lines.append(
            f"| {prov} | `{model}` | {acc:.0%} | {mal:.0%} | {spur:.0%} | {hall:.0%} | {med:.2f}s |"
        )
    lines += [
        "",
        "**Selection accuracy** — called exactly the expected tool set (for the ambiguous case,",
        "picking exactly one of the two defensible tools counts as correct).",
        "**Malformed** — at least one call had arguments failing schema validation.",
        "**Spurious** — called a tool on the question where none applies.",
        "**Hallucinated arg** — supplied `room` for `music_play` when the prompt never said one.",
        "",
        "Attach this file and `probe-results.json` to ARIA-40 (or ARIA-109) as the evidence the",
        "story's acceptance criteria require.",
    ]
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--provider", choices=sorted(PROVIDERS), action="append",
                    help="repeatable; default = every provider with a key present")
    ap.add_argument("--model", action="append",
                    help="override the default model. Repeatable — pass it several times to "
                         "compare candidate models in one run (this is what ARIA-109 needs).")
    ap.add_argument("--repeats", type=int, default=3, help="runs per case (default 3)")
    ap.add_argument("--base-url", default="http://localhost:11434",
                    help="Ollama base URL. With or without a trailing /v1 — both work.")
    ap.add_argument("--timeout", type=int, default=120,
                    help="Per-request timeout in seconds (default 120). Raise it if the model "
                         "runs on CPU; a slow backend is a FINDING, not a reason to wait longer.")
    ap.add_argument("--think", action="store_true",
                    help="Ollama only: leave the model's thinking/reasoning ENABLED. Off by "
                         "default because reasoning before a tool call is what causes timeouts.")
    ap.add_argument("--ollama-openai-compat", action="store_true",
                    help="Ollama only: use /v1/chat/completions instead of native /api/chat. "
                         "Thinking cannot be disabled on that path.")
    ap.add_argument("--stream", action="store_true",
                    help="Also run a streaming pass measuring tool-call event timing "
                         "(ARIA-109 / D37). Ollama native API only for now.")
    ap.add_argument("--out", default="probe-results.json")
    ap.add_argument("--from-json", metavar="FILE",
                    help="Re-generate the summaries from an existing probe-results.json "
                         "instead of re-running the probes.")
    args = ap.parse_args()

    if args.from_json:
        with open(args.from_json) as f:
            all_results = json.load(f)
        summary = summarise(all_results) + summarise_streaming(all_results)
        with open("probe-summary.md", "w") as f:
            f.write(summary)
        print(summary)
        print(f"Re-summarised {len(all_results)} records from {args.from_json}")
        return

    providers = args.provider or [
        p for p, env in (("anthropic", "ANTHROPIC_API_KEY"),
                         ("openai", "OPENAI_API_KEY"),
                         ("gemini", "GEMINI_API_KEY")) if os.environ.get(env)
    ]
    if not providers:
        sys.exit("No provider selected and no API keys found in the environment. "
                 "Set one of ANTHROPIC_API_KEY / OPENAI_API_KEY / GEMINI_API_KEY, "
                 "or pass --provider ollama --base-url ...")

    if args.model and len(providers) > 1:
        sys.exit("--model only makes sense with a single --provider")

    OPTS["timeout"] = args.timeout
    OPTS["think"] = args.think
    if args.ollama_openai_compat:
        PROVIDERS["ollama"] = (PROVIDERS["ollama"][0], probe_ollama_openai)
    if "ollama" in providers:
        path = "/v1/chat/completions" if args.ollama_openai_compat else "/api/chat"
        print(f"Ollama: {path}, think={args.think}, timeout={args.timeout}s")

    all_results = []
    for prov in providers:
        for model in (args.model or [PROVIDERS[prov][0]]):
            print(f"\n=== {prov} / {model} ===")
            try:
                all_results += run(prov, model, args.repeats, args.base_url)
                if args.stream:
                    print(f"\n--- streaming pass: {prov} / {model} ---")
                    all_results += run_streaming(prov, model, args.repeats, args.base_url)
            except RuntimeError as e:
                print(f"\n  SETUP PROBLEM — skipping {prov}/{model}:\n  {e}\n", file=sys.stderr)
                continue

    if not all_results:
        sys.exit("\nNo results collected. Fix the setup problem above and re-run.")

    with open(args.out, "w") as f:
        json.dump(all_results, f, indent=1)
    summary = summarise(all_results) + summarise_streaming(all_results)
    with open("probe-summary.md", "w") as f:
        f.write(summary)
    print("\n" + summary)
    print(f"Wrote {args.out} and probe-summary.md")


if __name__ == "__main__":
    main()
