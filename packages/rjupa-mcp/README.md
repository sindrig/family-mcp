# rjupa-mcp

Read-only ptarmigan hanging calculator for [rjupa.irdn.is](https://rjupa.irdn.is).
Native Rust executable using the same shared stdio MCP runtime and release tooling
as the other family MCPs. No account, API key, cookies or saved session required.
Initial unreleased preview: `rjupa-mcp@0.1.0`. Not affiliated with the calculator
or its weather providers.

## Build and configure

This package is new; no published release is assumed. Build from this fork with
the pinned Rust toolchain:

```sh
cd rust
cargo build --release --locked -p rjupa-mcp
./target/release/rjupa-mcp --version
```

Register the absolute executable path in your MCP client:

```json
{
  "mcpServers": {
    "rjupa": {
      "command": "/absolute/path/to/family-mcp/rust/target/release/rjupa-mcp",
      "args": ["serve"]
    }
  }
}
```

Claude Code:

```sh
claude mcp add rjupa -- /absolute/path/to/rjupa-mcp serve
```

Codex:

```toml
[mcp_servers.rjupa]
command = "/absolute/path/to/rjupa-mcp"
args = ["serve"]
```

After a maintainer publishes the matching release in **sindrig/family-mcp**,
the shared checksum-verifying installer will support macOS/Linux arm64/x64:

```sh
curl -fsSL https://raw.githubusercontent.com/sindrig/family-mcp/rjupa-mcp@0.1.0/packages/rjupa-mcp/install.sh | sh
```

## Tool

`get_degree_days` is annotated read-only. Its HTTP POST performs a public
calculation, not an account mutation. The tool sends only the chosen date range
and approximate coordinates to `https://rjupa.irdn.is/api/daggradur`; notification
state is never sent. This is the site's **internal, unversioned** API.

| Input                     | Meaning                                                                                          |
| ------------------------- | ------------------------------------------------------------------------------------------------ |
| `start_date`              | Required `YYYY-MM-DD` hanging date; starts at **00:00 UTC**.                                     |
| `latitude`, `longitude`   | Required, explicitly selected approximate location. No implicit home location.                   |
| `end_date`                | Inclusive date, default today UTC; no future dates; at most 366 days including both endpoints.   |
| `last_notified_threshold` | Default `30` means none sent. Otherwise the last delivered milestone, a multiple of 5 from `35`. |

Example (illustrative date and Reykjavík coordinates, not a registered batch):

```json
{
  "start_date": "2026-10-20",
  "latitude": 64.1466,
  "longitude": -21.9426,
  "last_notified_threshold": 30
}
```

Each completed UTC hourly sample contributes `max(temperature °C, 1) / 24`.
The site's rounded display total is preserved as `site_total`; `degree_days`
is independently calculated from completed hourly rows without rounding.
Current and future hours are excluded, so `degree_days` can differ from the
site's total if it includes those hours. Date-only input cannot represent an
exact hanging time; explain the midnight assumption to the user.

Results include `source`, UTC `fetched_at`/`checked_at`, `attribution`, location,
station, quality counts, coverage, `reminders_ready`, `pending_thresholds` and
`next_threshold`. Milestones are **35, 40, 45, 50, 55… °C·days**, not instantaneous
°C. A jump from 34 to 46 returns `[35, 40, 45]`; acknowledging 45 leaves the next
milestone at 50. A later downward weather revision never resets a delivered
cursor. `next_threshold` is the next undelivered milestone after pending ones.

Coverage must contain every completed hour from the start through the requested
end (or the current UTC hour boundary), with consistent quality counts. Missing,
stale or inconsistent coverage returns `reminders_ready: false` and **null**
pending/next thresholds: unknown, not no reminders. Malformed responses, duplicate
hours, wrong request echoes, unsupported source shapes or inconsistent totals
fail with a sanitized MCP error. No retry, cache or cursor mutation occurs inside
the MCP; normal host polling provides retries without hammering the site.

Quality preserves measured, interpolated and modeled hour counts. Open-Meteo
fallback and interpolation can trigger **estimated** milestones when coverage is
complete; include that qualification in the notification. Quote the returned
attribution (including **Veðurstofa Íslands**) and fetch time. Weather near a
location is not a measurement of actual hanging-room temperature, and a milestone
is not a food-safety determination.

## Assistant reminder workflow

The MCP supplies the calculation; the **host** supplies scheduling, private
per-batch durable state, notification delivery and cancellation. This separation
matches the read-only weather MCP. Starting this executable alone sends nothing.

1. When the user asks to track a batch, obtain its hanging date and approximate
   location. Explain the midnight assumption; do not silently borrow a location
   from unrelated family data. Associate a stable batch ID and explicitly chosen
   recipient with the request. Persist `active: true` and
   `last_notified_threshold: 30` in private host state.
2. Create an hourly scheduled check (Iceland is UTC year-round). Each run must
   load the latest batch state and skip inactive batches. Serialize runs for a
   batch; do not run overlapping sends. Keep date, coordinates and cursor separate
   for each batch. Never post a private batch into another channel by default.
3. Call `get_degree_days` with the saved date, coordinates and delivered cursor.
   If the call fails or `reminders_ready` is false, preserve the cursor. Report
   failure as unavailable, never as no threshold reached; retry next hour. Follow
   the host owner's configured error-alert destination.
4. If pending milestones exist, recheck that the batch is still active, then
   deliver one catch-up notification naming all newly reached milestones,
   degree-days, date/location, coverage, estimate quality, attribution and fetch
   time. Persist the highest milestone **only after successful delivery**. Use a
   host delivery receipt/idempotency key `(batch_id, milestone)` when available.
   A send with unknown outcome requires receipt reconciliation; do not blindly
   retry or falsely acknowledge it. The MCP itself does not promise exactly-once
   delivery across host crashes.
5. When the user says the birds are taken down, set that batch inactive and cancel
   its scheduled check. Resolve ambiguity if several batches are active. No more
   reminders at 45, 50… after cancellation. Retain the delivery ledger; starting a
   new batch must use a new batch ID rather than reusing the old cursor.

No actual batch, scheduler or notification destination is created by installation.

## Development

Tests are offline, using synthetic data and a loopback HTTP fake. No upstream
source code or captured weather response is redistributed. `test-origin` restricts
requests to an exact loopback origin and adds a deterministic clock; neither
hook is compiled into a shipping build.

```sh
cd rust
cargo fmt --check
cargo clippy --all-targets --all-features --locked -p rjupa-mcp -- -D warnings
cargo test --all-features --locked -p rjupa-mcp
cargo test --locked -p rjupa-mcp
```

The public contract investigation is in [API.md](docs/API.md); native release
checks are in [RELEASING.md](docs/RELEASING.md).
