<#
    ARIA - Ollama / GPU diagnosis, remote edition.

    Run this from Windows against the Ollama host over HTTP. It gets the decisive
    answer - is the model on GPU or CPU - without needing a shell on the container.

        .\aria-ollama-diagnose.ps1 -BaseUrl http://192.168.1.15:11434 -Model qwen3:8b

    What it CANNOT see remotely (needs aria-ollama-diagnose.sh in the container shell,
    or a session on the Proxmox host):
      - /dev/nvidia* device nodes        - nvidia-smi
      - Ollama's service logs            - lspci / dmesg / vfio binding

    Section 2 is the one that matters. Everything after it explains the result.
#>

[CmdletBinding()]
param(
    [string]$BaseUrl = "http://localhost:11434",
    [string]$Model   = "qwen3:8b",
    [int]$TimeoutSec = 180
)

$ErrorActionPreference = "Continue"
# Normalise: accept a trailing /v1 or / just like the probe does.
$BaseUrl = $BaseUrl.TrimEnd('/')
if ($BaseUrl.EndsWith('/v1')) { $BaseUrl = $BaseUrl.Substring(0, $BaseUrl.Length - 3) }

function Section($t) { Write-Host "`n`n=== $t ===`n" -ForegroundColor Cyan }
function Note($t)    { Write-Host $t -ForegroundColor Yellow }

function Invoke-Timed {
    param([string]$Url, [hashtable]$Body, [int]$Timeout = 180)
    $json = $Body | ConvertTo-Json -Depth 12 -Compress
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $r = Invoke-RestMethod -Uri $Url -Method Post -Body $json `
                -ContentType 'application/json' -TimeoutSec $Timeout -ErrorAction Stop
        $sw.Stop()
        return @{ ok = $true; seconds = $sw.Elapsed.TotalSeconds; data = $r }
    } catch {
        $sw.Stop()
        return @{ ok = $false; seconds = $sw.Elapsed.TotalSeconds; error = $_.Exception.Message }
    }
}

Write-Host "ARIA Ollama diagnosis (remote) - $(Get-Date -Format s)"
Write-Host "target: $BaseUrl   model: $Model   timeout: ${TimeoutSec}s"
Write-Host "PowerShell $($PSVersionTable.PSVersion)"

# ---------------------------------------------------------------------------------------
Section "1. REACHABILITY"
try {
    $v = Invoke-RestMethod -Uri "$BaseUrl/api/version" -TimeoutSec 15 -ErrorAction Stop
    Write-Host "  Ollama version: $($v.version)"
} catch {
    Write-Host "  CANNOT REACH $BaseUrl : $($_.Exception.Message)" -ForegroundColor Red
    Note "  Check the host/port, and that Ollama listens beyond localhost (OLLAMA_HOST=0.0.0.0)."
    exit 1
}

try {
    $tags = Invoke-RestMethod -Uri "$BaseUrl/api/tags" -TimeoutSec 30 -ErrorAction Stop
    Write-Host "  Models available:"
    foreach ($m in $tags.models) {
        $gb = [math]::Round($m.size / 1GB, 2)
        Write-Host ("    {0,-28} {1,8} GB   {2}" -f $m.name, $gb, $m.details.parameter_size)
    }
} catch { Write-Host "  /api/tags failed: $($_.Exception.Message)" }

# ---------------------------------------------------------------------------------------
Section "2. * IS THE MODEL ON GPU OR CPU? - the decisive check"
Note "  Loading the model first so there is something to inspect..."
$warm = Invoke-Timed -Url "$BaseUrl/api/chat" -Timeout $TimeoutSec -Body @{
    model = $Model; stream = $false; think = $false
    messages = @(@{ role = "user"; content = "Say OK." })
    options = @{ num_predict = 8 }
}
if (-not $warm.ok) {
    Write-Host "  Warm-up FAILED after $([math]::Round($warm.seconds,1))s: $($warm.error)" -ForegroundColor Red
    Note "  A timeout here already tells you a lot - see section 4."
} else {
    Write-Host "  Warm-up OK in $([math]::Round($warm.seconds,2))s"
}

try {
    $ps = Invoke-RestMethod -Uri "$BaseUrl/api/ps" -TimeoutSec 30 -ErrorAction Stop
    if (-not $ps.models -or $ps.models.Count -eq 0) {
        Write-Host "  No model resident. If warm-up failed, that is consistent." -ForegroundColor Red
    }
    foreach ($m in $ps.models) {
        $size = [double]$m.size
        $vram = [double]$m.size_vram
        $pct  = if ($size -gt 0) { [math]::Round(100 * $vram / $size) } else { 0 }
        $verdict = if ($pct -ge 99) { "100% GPU" }
                   elseif ($pct -le 1) { "100% CPU" }
                   else { "$pct% GPU / $(100-$pct)% CPU (partial offload)" }
        $colour = if ($pct -ge 99) { "Green" } elseif ($pct -le 1) { "Red" } else { "Yellow" }
        Write-Host ""
        Write-Host ("  {0}" -f $m.name)
        Write-Host ("    total  : {0} GB" -f [math]::Round($size/1GB,2))
        Write-Host ("    in VRAM: {0} GB" -f [math]::Round($vram/1GB,2))
        Write-Host ("    PROCESSOR: {0}" -f $verdict) -ForegroundColor $colour
    }
} catch { Write-Host "  /api/ps failed: $($_.Exception.Message)" }

Note @"

  >>> 100% GPU  -> passthrough is FINE. Your timeout was thinking. See section 3.
  >>> 100% CPU  -> PASSTHROUGH IS BROKEN. This is the timeout. See section 5.
  >>> partial   -> model does not fit in free VRAM. Something else holds it,
  >>>             or the card is smaller than the model. Also see section 5.
"@

# ---------------------------------------------------------------------------------------
Section "3. THINKING vs NOT - is reasoning the cause?"
$a = Invoke-Timed -Url "$BaseUrl/api/chat" -Timeout $TimeoutSec -Body @{
    model = $Model; stream = $false; think = $false
    messages = @(@{ role = "user"; content = "Say OK and nothing else." })
    options = @{ num_predict = 16 }
}
if ($a.ok) {
    $tps = if ($a.data.total_duration) { [math]::Round($a.data.eval_count / ($a.data.total_duration/1e9), 1) } else { "?" }
    Write-Host ("  3a think=false : {0,7:N2}s   {1} tokens   {2} tok/s" -f $a.seconds, $a.data.eval_count, $tps)
} else {
    Write-Host ("  3a think=false : FAILED after {0:N1}s - {1}" -f $a.seconds, $a.error) -ForegroundColor Red
}

$b = Invoke-Timed -Url "$BaseUrl/api/chat" -Timeout $TimeoutSec -Body @{
    model = $Model; stream = $false
    messages = @(@{ role = "user"; content = "Say OK and nothing else." })
    options = @{ num_predict = 512 }
}
if ($b.ok) {
    $tps = if ($b.data.total_duration) { [math]::Round($b.data.eval_count / ($b.data.total_duration/1e9), 1) } else { "?" }
    Write-Host ("  3b think=deflt : {0,7:N2}s   {1} tokens   {2} tok/s" -f $b.seconds, $b.data.eval_count, $tps)
    if ($b.data.message.thinking) {
        Write-Host "     -> model DID emit a thinking block" -ForegroundColor Yellow
    }
} else {
    Write-Host ("  3b think=deflt : FAILED after {0:N1}s - {1}" -f $b.seconds, $b.error) -ForegroundColor Red
}

Note @"

  >>> 3a fast, 3b slow/failed -> THINKING was the timeout, not the GPU.
  >>> both slow (<10 tok/s)   -> CPU inference. Passthrough. Section 5.
  >>> both fast               -> neither; the tool schemas are the trigger. Section 4.
"@

# ---------------------------------------------------------------------------------------
Section "4. WITH TOOL SCHEMAS - what the probe actually sends"
$tools = @(
    @{ type = "function"; function = @{
        name = "music_play"; description = "Play music on a speaker in the house."
        parameters = @{ type = "object"
            properties = @{ query = @{ type = "string" }; room = @{ type = "string" } }
            required = @("query","room") } } },
    @{ type = "function"; function = @{
        name = "light_set"; description = "Turn a light on or off."
        parameters = @{ type = "object"
            properties = @{ room = @{ type = "string" }; on = @{ type = "boolean" } }
            required = @("room","on") } } }
)
$c = Invoke-Timed -Url "$BaseUrl/api/chat" -Timeout $TimeoutSec -Body @{
    model = $Model; stream = $false; think = $false; tools = $tools
    messages = @(@{ role = "user"; content = "Play some jazz in the kitchen." })
    options = @{ num_predict = 256 }
}
if ($c.ok) {
    $tps = if ($c.data.total_duration) { [math]::Round($c.data.eval_count / ($c.data.total_duration/1e9), 1) } else { "?" }
    Write-Host ("  with tools     : {0,7:N2}s   {1} tok/s" -f $c.seconds, $tps)
    $tc = $c.data.message.tool_calls
    if ($tc) {
        Write-Host "  TOOL CALL RETURNED:" -ForegroundColor Green
        $tc | ConvertTo-Json -Depth 8
        Note "  >>> Native tool calling works. The probe will run."
    } else {
        Write-Host "  NO tool call returned. Text was:" -ForegroundColor Yellow
        Write-Host "  $($c.data.message.content)"
        Note "  >>> Model may not support native tool calling. That is an ARIA-109 FINDING,"
        Note "  >>> not a bug - record it and try another model."
    }
} else {
    Write-Host ("  with tools     : FAILED after {0:N1}s - {1}" -f $c.seconds, $c.error) -ForegroundColor Red
}

# ---------------------------------------------------------------------------------------
Section "5. NEXT STEPS BY RESULT"
Note @"
  IF 100% CPU (passthrough broken) - run in the CONTAINER shell:
      ls -l /dev/nvidia*            # missing => not passed through
      nvidia-smi                    # missing/erroring => driver problem
      journalctl -u ollama -n 40    # look for "no compatible GPUs were discovered"

  ... and on the PROXMOX HOST:
      lspci -nnk | grep -A3 -i nvidia
        "Kernel driver in use: nvidia"   correct for LXC, WRONG for a VM
        "Kernel driver in use: vfio-pci" correct for a VM, WRONG for LXC
        "Kernel driver in use: nouveau"  blacklist nouveau
      ls -l /dev/nvidia*              # note the MAJOR numbers
      pct config <CTID>               # LXC: do the cgroup2 allow lines match those majors?
      qm config <VMID> | grep hostpci # VM: is the card still assigned?
      dmesg | grep -i -e vfio -e nvidia | tail -30

  >>> A power-cycle can renumber /dev/nvidia* device majors. The lxc.cgroup2.devices.allow
  >>> lines then point at the wrong device and Ollama silently falls back to CPU -
  >>> which looks exactly like your symptom: API answers, generation crawls.

  IF thinking was the cause:
      Nothing to fix on the host. The probe already sends think=false by default.
      Re-run it - and record that this model reasons before tool calls (ARIA-54/ARIA-61).

  EITHER WAY: record the outcome on ARIA-27 (VRAM) and ARIA-79 (GPU runtime).
"@

Write-Host "`nDone. Copy this output back into the session.`n"
