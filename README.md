# Funliday CLI

> Experimental, unofficial command-line client for [Funliday](https://www.funliday.com/).
> Not affiliated with Funliday.

The goal is to let local AI agents such as Hermes manage a user's personal
Funliday itineraries through a small CLI and a safety-gated MCP server.

## Status

This repository is in active API-discovery stage. Browser-assisted login,
personal-trip reads, trip creation/deletion, and a safety-gated MCP server are
implemented. Trip metadata and itinerary-item editing are still being validated.

The older public developer portal exists at `api.funliday.com`, but currently
returns a Heroku application error. Funliday's separate public Data API exposes
aggregate travel datasets, not personal itinerary management.

## Build and try

```bash
cargo build --release
./target/release/funliday login
./target/release/funliday status
./target/release/funliday trips list
./target/release/funliday trips show <trip-id>
./target/release/funliday capabilities
```

Create and delete operations require an explicit confirmation flag. Creation
currently accepts the Funliday city ID returned by its autocomplete service:

```bash
funliday trips create --name "Fukuoka" --city 7868657 \
  --start 2026-10-20 --end 2026-10-21 --trip-type 1 --yes
funliday trips delete <container-id> --yes
```

`trips list` returns both the itinerary `_id` and the `containerId`; deletion
uses the latter.

## MCP server

The stdio MCP server is read-only by default:

```bash
funliday mcp
```

It exposes `list_trips` and `get_trip`. To advertise `create_trip` and
`delete_trip`, the host must start `funliday mcp --enable-write`; every write
call must additionally include `confirm: true`.

Login opens Funliday's official page in an isolated Chrome profile. Passwords and
social-login credentials stay on that page. The resulting session token is stored
in the operating system's standard config directory as a mode-`0600` file on Unix;
the project does not require macOS Keychain.

Do not commit the local config directory, browser profile, tokens, captured
requests, or personal itinerary exports.
