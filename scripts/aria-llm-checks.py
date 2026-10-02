#!/usr/bin/env python3
"""
ARIA-122 follow-up checks — run after aria-llm-probe.py.

Re-uses the probe's five tools, five cases and type table, and adds what the first
round could not measure. Each variant runs the full battery (5 cases x --repeats):

  dated    The system prompt carries today's date, time and time zone. The
           "tomorrow at 3pm" case is checked for the right date.
  clarify  dated + a `request_information` tool. On the missing-room case,
           asking is the expected answer.
  strict   dated + strict tool schemas (constrained decoding where the provider
           supports it). Null-valued optional arguments are stripped before
           validation and counted.
  stream   dated, streamed: time to first token (first chunk carrying text or a
           tool call), time to the first tool call, and whether tool arguments
           arrive in pieces.

Profiles: cerebras, openai, anthropic, openrouter. Any other OpenAI-compatible host:
--profile openai --base-url <url> --key-env <VAR>. Standard library only.

Usage
-----
    python scripts/aria-llm-checks.py --profile cerebras  --model qwen-3.8-27b
    python scripts/aria-llm-checks.py --profile cerebras  --model qwen-3.8-27b --reasoning none --tag noreason --variants dated,stream
    python scripts/aria-llm-checks.py --profile openai    --model gpt-6-sol
    python scripts/aria-llm-checks.py --profile anthropic --model claude-sonnet-5-5

Writes <out-dir>/checks-<profile>-<model>[-<tag>].json and .md. The JSON is rewritten
after every variant, so an interrupted run keeps the variants already finished.
"""

import argparse
import copy
import datetime as dt
import importlib.util
import json
import os
import statistics
import sys
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("aria_llm_probe", os.path.join(HERE, "aria-llm-probe.py"))
probe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(probe)

UA = "aria-llm-checks/1.0"  # Groq/Cloudflare refuse Python's default user agent (error 1010)

PROFILES = {
    "cerebras": dict(kind="openai", base="https://api.cerebras.ai/v1", key="CEREBRAS_API_KEY",
                     pause=13.0, max_field="max_completion_tokens", reasoning=None),
    "openai": dict(kind="openai", base="https://api.openai.com/v1", key="OPENAI_API_KEY",
                   pause=1.0, max_field="max_completion_tokens",
                   # GPT-6 Sol: Chat Completions rejects function tools while reasoning is on.
                   reasoning="none"),
    "openrouter": dict(kind="openai", base="https://openrouter.ai/api/v1", key="OPENROUTER_API_KEY",
                       pause=3.5, max_field="max_tokens", reasoning=None,
                       extra={"provider": {"zdr": True, "data_collection": "deny",
                                           "require_parameters": True}}),
    "anthropic": dict(kind="anthropic", base="https://api.anthropic.com/v1", key="ANTHROPIC_API_KEY",
                      pause=1.0, reasoning=None),
}

CLARIFY_TOOL = {
    "name": "request_information",
    "description": "Ask the user for information a request needs but did not include. "
                   "Use this instead of guessing a value.",
    "parameters": {
        "type": "object",
        "properties": {"question": {"type": "string", "description": "The question to ask"}},
        "required": ["question"],
        "additionalProperties": False,
    },
}

ALL_VARIANTS = ["dated", "clarify", "strict", "stream"]


# --------------------------------------------------------------------------------------
# Battery
# --------------------------------------------------------------------------------------

def date_context(tz_name):
    now = dt.datetime.now().astimezone()  # OS local time; avoids needing tzdata on Windows
    off = now.strftime("%z")
    offset = f"UTC{off[:3]}:{off[3:]}"
    line = (f"Current date and time: {now:%A, %Y-%m-%d %H:%M}, "
            f"time zone {tz_name} ({offset}).")
    tomorrow = (now + dt.timedelta(days=1)).date().isoformat()
    return line, f"{tomorrow}T15:00"


def battery(variant, date_line):
    tools = list(probe.TOOLS) + ([CLARIFY_TOOL] if variant == "clarify" else [])
    cases = copy.deepcopy(probe.CASES)
    if variant == "clarify":
        for c in cases:
            if c["id"] == "missing_arg":
                c["expected"] = {"request_information"}
    system = f"{probe.SYSTEM} {date_line}"
    return system, tools, cases


def strict_params(params, style):
    """openai style: every property required, optional ones nullable (OpenAI/Cerebras rule).
    anthropic style: schema unchanged (already additionalProperties: false)."""
    p = copy.deepcopy(params)
    if style == "anthropic":
        return p
    required = set(p.get("required", []))
    for name, prop in p["properties"].items():
        if name not in required:
            prop["type"] = [prop["type"], "null"]
    p["required"] = list(p["properties"])
    p["additionalProperties"] = False
    return p


def validate(tool_by_name, name, args, strip_nulls):
    """Same rules as the probe's validate_args, extended to the clarify tool.
    Returns (problems, nulls_stripped, args_after_stripping)."""
    tool = tool_by_name.get(name)
    if tool is None:
        return [f"unknown tool {name!r}"], 0, args
    if not isinstance(args, dict):
        return [f"arguments not an object: {type(args).__name__}"], 0, args
    schema = tool["parameters"]
    props, required = schema["properties"], schema.get("required", [])
    stripped = 0
    if strip_nulls:
        kept = {}
        for k, v in args.items():
            if v is None and k not in required:
                stripped += 1
            else:
                kept[k] = v
        args = kept
    problems = [f"missing required {r!r}" for r in required if r not in args]
    for key, val in args.items():
        if key not in props:
            problems.append(f"unexpected property {key!r}")
            continue
        want = props[key].get("type")
        py = probe._TYPES.get(want)
        if py and not isinstance(val, py):
            problems.append(f"{key!r} should be {want}, got {type(val).__name__}")
        if isinstance(val, bool) and want == "integer":
            problems.append(f"{key!r} should be integer, got boolean")
    return problems, stripped, args


# --------------------------------------------------------------------------------------
# HTTP
# --------------------------------------------------------------------------------------

def open_request(url, body, headers, timeout):
    """POST with up to 3 retries on 429/503/529 (honours retry-after). Returns an open response."""
    data = json.dumps(body).encode()
    for attempt in range(4):
        req = urllib.request.Request(url, data=data, method="POST")
        req.add_header("Content-Type", "application/json")
        req.add_header("User-Agent", UA)
        for k, v in headers.items():
            req.add_header(k, v)
        try:
            return urllib.request.urlopen(req, timeout=timeout)
        except urllib.error.HTTPError as e:
            detail = e.read().decode(errors="replace")[:600]
            if e.code in (429, 503, 529) and attempt < 3:
                wait = float(e.headers.get("retry-after") or 20)
                print(f"    HTTP {e.code}, waiting {wait:.0f}s and retrying", file=sys.stderr)
                time.sleep(wait)
                continue
            raise RuntimeError(f"HTTP {e.code}: {detail}") from None
        except urllib.error.URLError as e:
            raise RuntimeError(f"cannot reach {url}: {e.reason}") from None


def parse_args_json(raw):
    if isinstance(raw, dict):
        return raw
    if raw in (None, ""):
        return {}
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {"__unparseable__": raw}


# --------------------------------------------------------------------------------------
# Provider adapters -> {"calls": [(name, args)], "seconds", stream facts...}
# --------------------------------------------------------------------------------------

def openai_body(cfg, a, system, tools, prompt, strict, stream):
    tool_defs = []
    for t in tools:
        fn = {"name": t["name"], "description": t["description"],
              "parameters": strict_params(t["parameters"], "openai") if strict else t["parameters"]}
        if strict:
            fn["strict"] = True
        tool_defs.append({"type": "function", "function": fn})
    body = {"model": a.model,
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
            "tools": tool_defs, cfg["max_field"]: a.max_tokens}
    body.update(cfg.get("extra", {}))
    if a.reasoning_effective:
        body["reasoning_effort"] = a.reasoning_effective
    if stream:
        body["stream"] = True
    return body


def call_openai(cfg, a, system, tools, prompt, strict, stream):
    url = f"{a.base}/chat/completions"
    headers = {"Authorization": f"Bearer {os.environ.get(a.key_env, '')}"}
    body = openai_body(cfg, a, system, tools, prompt, strict, stream)
    t0 = time.monotonic()
    resp = open_request(url, body, headers, a.timeout)
    if not stream:
        data = json.loads(resp.read().decode())
        msg = data["choices"][0]["message"]
        calls = [(tc["function"]["name"], parse_args_json(tc["function"].get("arguments")))
                 for tc in (msg.get("tool_calls") or [])]
        return {"calls": calls, "seconds": time.monotonic() - t0, "usage": data.get("usage", {})}
    first_text = first_tool = None
    acc = {}
    for raw in resp:
        line = raw.decode(errors="replace").strip()
        if not line.startswith("data:"):
            continue
        payload = line[5:].strip()
        if payload == "[DONE]":
            break
        chunk = json.loads(payload)
        if not chunk.get("choices"):
            continue
        delta = chunk["choices"][0].get("delta") or {}
        now = time.monotonic() - t0
        if delta.get("content") and first_text is None:
            first_text = now
        for tc in delta.get("tool_calls") or []:
            e = acc.setdefault(tc.get("index", 0), {"name": "", "args": "", "chunks": 0})
            fn = tc.get("function") or {}
            if fn.get("name"):
                e["name"] += fn["name"]
                if first_tool is None:
                    first_tool = now
            if fn.get("arguments"):
                e["args"] += fn["arguments"]
                e["chunks"] += 1
    total = time.monotonic() - t0
    calls = [(e["name"], parse_args_json(e["args"])) for _, e in sorted(acc.items())]
    return stream_facts(calls, total, first_text, first_tool, [e["chunks"] for e in acc.values()])


def call_anthropic(cfg, a, system, tools, prompt, strict, stream):
    url = f"{a.base}/messages"
    headers = {"x-api-key": os.environ.get(a.key_env, ""), "anthropic-version": "2023-06-01"}
    tool_defs = []
    for t in tools:
        d = {"name": t["name"], "description": t["description"],
             "input_schema": strict_params(t["parameters"], "anthropic") if strict else t["parameters"]}
        if strict:
            d["strict"] = True
        tool_defs.append(d)
    body = {"model": a.model, "max_tokens": a.max_tokens, "system": system,
            "messages": [{"role": "user", "content": prompt}], "tools": tool_defs}
    if stream:
        body["stream"] = True
    t0 = time.monotonic()
    resp = open_request(url, body, headers, a.timeout)
    if not stream:
        data = json.loads(resp.read().decode())
        calls = [(b["name"], b.get("input", {})) for b in data.get("content", []) if b.get("type") == "tool_use"]
        return {"calls": calls, "seconds": time.monotonic() - t0, "usage": data.get("usage", {})}
    first_text = first_tool = None
    blocks = {}
    for raw in resp:
        line = raw.decode(errors="replace").strip()
        if not line.startswith("data:"):
            continue
        ev = json.loads(line[5:].strip())
        now = time.monotonic() - t0
        typ = ev.get("type")
        if typ == "content_block_start":
            cb = ev.get("content_block", {})
            if cb.get("type") == "tool_use":
                blocks[ev["index"]] = {"name": cb.get("name", ""), "args": "", "chunks": 0}
                if first_tool is None:
                    first_tool = now
        elif typ == "content_block_delta":
            d = ev.get("delta", {})
            if d.get("type") == "text_delta" and d.get("text") and first_text is None:
                first_text = now
            elif d.get("type") == "input_json_delta" and ev["index"] in blocks:
                blocks[ev["index"]]["args"] += d.get("partial_json", "")
                blocks[ev["index"]]["chunks"] += 1
        elif typ == "message_stop":
            break
    total = time.monotonic() - t0
    calls = [(b["name"], parse_args_json(b["args"])) for _, b in sorted(blocks.items())]
    return stream_facts(calls, total, first_text, first_tool, [b["chunks"] for b in blocks.values()])


def stream_facts(calls, total, first_text, first_tool, chunk_counts):
    firsts = [x for x in (first_text, first_tool) if x is not None]
    return {"calls": calls, "seconds": total,
            "ttft": min(firsts) if firsts else None,
            "first_tool": first_tool,
            "text_before_tool": (first_text is not None and first_tool is not None and first_text < first_tool),
            "arg_chunks": chunk_counts}


# --------------------------------------------------------------------------------------
# Runner
# --------------------------------------------------------------------------------------

def run_variant(cfg, a, variant, date_line, expected_date):
    system, tools, cases = battery(variant, date_line)
    tool_by_name = {t["name"]: t for t in tools}
    strict, stream = variant == "strict", variant == "stream"
    fn = call_anthropic if cfg["kind"] == "anthropic" else call_openai
    out = []
    for case in cases:
        for i in range(a.repeats):
            time.sleep(a.pause)
            rec = {"profile": a.profile, "model": a.model, "variant": variant, "tag": a.tag,
                   "case": case["id"], "run": i}
            try:
                r = fn(cfg, a, system, tools, case["prompt"], strict, stream)
            except Exception as e:  # noqa: BLE001 - report and continue
                rec["error"] = str(e)[:600]
                out.append(rec)
                print(f"  {variant:7} {case['id']:12} run {i}  ERROR  {str(e)[:110]}", file=sys.stderr)
                continue
            names, problems, nulls, calls = [], [], 0, []
            for name, args in r["calls"]:
                p, n, args = validate(tool_by_name, name, args, strip_nulls=strict)
                problems += [f"{name}: {x}" for x in p]
                nulls += n
                names.append(name)
                calls.append({"name": name, "args": args})
            called, expected = set(names), case["expected"]
            if not expected:
                correct = not called
            elif case["id"] == "ambiguous":
                correct = len(called) == 1 and called <= expected
            else:
                correct = called == expected
            hallucinated = None
            if case.get("watch_hallucination"):
                tname, argname = case["watch_hallucination"]
                hallucinated = any(c["name"] == tname and argname in c["args"] for c in calls)
            date_status = date_value = None
            if case["id"] == "ambiguous" and calls:
                date_value = calls[0]["args"].get("due") or calls[0]["args"].get("start")
                if not date_value:
                    date_status = "missing"
                elif str(date_value).startswith(expected_date):
                    date_status = "correct"
                else:
                    date_status = "wrong"
            rec.update(calls=calls, tool_names=names, correct=correct, malformed=bool(problems),
                       problems=problems, nulls_stripped=nulls,
                       spurious=(not expected and bool(called)), hallucinated_arg=hallucinated,
                       refused=(case["id"] == "missing_arg" and not called),
                       date_status=date_status, date_value=date_value,
                       seconds=round(r["seconds"], 3), usage=r.get("usage"))
            for k in ("ttft", "first_tool"):
                if r.get(k) is not None:
                    rec[k] = round(r[k], 3)
            if stream:
                rec["text_before_tool"] = r["text_before_tool"]
                rec["arg_chunks"] = r["arg_chunks"]
            out.append(rec)
            flag = "ok  " if correct else "MISS"
            extra = (" MALFORMED" if problems else "") + (f" date={date_status}" if date_status else "")
            if stream and rec.get("ttft") is not None:
                extra += f" ttft={rec['ttft']:.2f}s chunks={r['arg_chunks']}"
            print(f"  {variant:7} {case['id']:12} run {i}  {flag} {names or '[]'}{extra}  {r['seconds']:.2f}s")
    return out


def med(xs):
    xs = [x for x in xs if x is not None]
    return f"{statistics.median(xs):.2f} s" if xs else "—"


def summarise(records, a, date_line):
    lines = [f"# ARIA-122 follow-up checks — {a.profile} · `{a.model}`" + (f" ({a.tag})" if a.tag else ""),
             "", f"Generated {dt.datetime.now():%Y-%m-%d %H:%M}. Repeats per case: {a.repeats}. "
             f"Reasoning sent: `{a.reasoning_effective or 'not set'}`. Date line: *{date_line}*", "",
             "| Variant | Runs ok | Correct | `missing_arg`: refused / asked | `multi` | Malformed | "
             "Nulls stripped | Spurious | Made-up room | Date correct | Median total | Median TTFT | "
             "Median first tool | Tool args in pieces |",
             "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for v in ALL_VARIANTS:
        rs = [r for r in records if r["variant"] == v]
        if not rs:
            continue
        ok = [r for r in rs if "error" not in r]
        ma = [r for r in ok if r["case"] == "missing_arg"]
        asked = sum(1 for r in ma if "request_information" in r["tool_names"])
        amb = [r for r in ok if r["case"] == "ambiguous" and r.get("date_status")]
        pieces = [r for r in ok if r.get("arg_chunks")]
        lines.append(
            f"| {v} | {len(ok)}/{len(rs)} | {sum(r['correct'] for r in ok)}/{len(ok)} | "
            f"{sum(r['refused'] for r in ma)} / {asked} | "
            f"{sum(r['correct'] for r in ok if r['case'] == 'multi')}/{sum(1 for r in ok if r['case'] == 'multi')} | "
            f"{sum(r['malformed'] for r in ok)} | {sum(r['nulls_stripped'] for r in ok)} | "
            f"{sum(r['spurious'] for r in ok)} | {sum(bool(r['hallucinated_arg']) for r in ok)} | "
            f"{sum(r['date_status'] == 'correct' for r in amb)}/{len(amb)} | "
            f"{med(r['seconds'] for r in ok)} | {med(r.get('ttft') for r in ok) if v == 'stream' else '—'} | "
            f"{med(r.get('first_tool') for r in ok) if v == 'stream' else '—'} | "
            + (f"{sum(1 for r in pieces if max(r['arg_chunks']) > 1)}/{len(pieces)}" if v == "stream" else "—")
            + " |")
    lines += ["", "**Correct** — same rule as the probe. In `clarify`, calling `request_information` "
              "is the expected answer to `missing_arg`. **Date correct** — of the `ambiguous` runs that "
              "made a call, how many wrote tomorrow at 15:00. **Nulls stripped** — `strict` only: "
              "null optional arguments removed before validation. **TTFT** — first streamed chunk "
              "carrying text or a tool call. **Tool args in pieces** — streamed runs whose tool "
              "arguments arrived in more than one chunk."]
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--profile", required=True, choices=sorted(PROFILES))
    ap.add_argument("--model", required=True)
    ap.add_argument("--variants", default=",".join(ALL_VARIANTS),
                    help=f"comma-separated subset of {','.join(ALL_VARIANTS)}")
    ap.add_argument("--repeats", type=int, default=5)
    ap.add_argument("--reasoning", help="reasoning_effort to send (OpenAI-compatible only); "
                                        "'omit' sends none. Default: the profile's setting")
    ap.add_argument("--tag", default="", help="suffix for output files, e.g. noreason")
    ap.add_argument("--pause", type=float, help="seconds before each request (default: profile)")
    ap.add_argument("--base-url", help="override the profile's base URL (include /v1)")
    ap.add_argument("--key-env", help="override the profile's API-key variable")
    ap.add_argument("--tz-name", default="Europe/Zurich", help="time-zone label in the date line")
    ap.add_argument("--max-tokens", type=int, default=1024)
    ap.add_argument("--timeout", type=int, default=120)
    ap.add_argument("--out-dir", default=os.path.join("docs", "spikes", "probe-runs-2026-10", "checks"))
    a = ap.parse_args()

    cfg = PROFILES[a.profile]
    a.base = (a.base_url or cfg["base"]).rstrip("/")
    a.key_env = a.key_env or cfg["key"]
    a.pause = cfg["pause"] if a.pause is None else a.pause
    a.reasoning_effective = None if a.reasoning == "omit" else (a.reasoning or cfg.get("reasoning"))
    if cfg["kind"] == "anthropic" and a.reasoning_effective:
        sys.exit("--reasoning applies to OpenAI-compatible profiles only")
    if not os.environ.get(a.key_env):
        sys.exit(f"{a.key_env} is not set")
    variants = [v.strip() for v in a.variants.split(",") if v.strip()]
    bad = [v for v in variants if v not in ALL_VARIANTS]
    if bad:
        sys.exit(f"unknown variant(s): {bad}")

    date_line, expected_date = date_context(a.tz_name)
    os.makedirs(a.out_dir, exist_ok=True)
    stem = f"checks-{a.profile}-{a.model.replace('/', '_').replace(':', '_')}" + (f"-{a.tag}" if a.tag else "")
    jpath, mpath = os.path.join(a.out_dir, stem + ".json"), os.path.join(a.out_dir, stem + ".md")

    print(f"{a.profile} / {a.model} — variants {variants}, {a.repeats} repeats, pause {a.pause}s, "
          f"reasoning {a.reasoning_effective or 'not set'}\n{date_line}  (expects {expected_date})")
    records = []
    for v in variants:
        print(f"\n=== {v} ===")
        records += run_variant(cfg, a, v, date_line, expected_date)
        with open(jpath, "w") as f:
            json.dump(records, f, indent=1, default=list)
    summary = summarise(records, a, date_line)
    with open(mpath, "w", encoding="utf-8") as f:
        f.write(summary)
    print("\n" + summary)
    print(f"Wrote {jpath} and {mpath}")


if __name__ == "__main__":
    main()
