# Funliday CLI

> Experimental, unofficial command-line client for [Funliday](https://www.funliday.com/).
> Not affiliated with Funliday.

The goal is to let local AI agents such as Hermes manage a user's personal
Funliday itineraries through a small CLI and a safety-gated MCP server.

## Status

Browser-assisted login, personal-trip reads, trip creation/deletion, place
search, custom places, fixed times, travel durations, notes, and a safety-gated
MCP server are implemented.

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
./target/release/funliday cities search 福岡
./target/release/funliday places search 櫛田神社
./target/release/funliday capabilities
```

Create and delete operations require an explicit confirmation flag. Creation
currently accepts the Funliday city ID returned by its autocomplete service:

```bash
funliday trips create --name "Fukuoka" --city 7868657 \
  --start 2026-10-20 --end 2026-10-21 --trip-type 1 --yes
funliday trips delete <container-id> --yes
funliday trips add-place <trip-id> --day 1 --poi <poi-id> --name 櫛田神社 \
  --latitude 33.59295 --longitude 130.41046 --stay-minutes 45 --yes
funliday trips add-custom-place <trip-id> --day 1 --name 飯店 \
  --address "Fukuoka, Japan" --latitude 33.59 --longitude 130.40 --yes
funliday trips set-place-time <trip-id> <item-id> \
  --start 11:00 --stay-minutes 60 --yes
funliday trips set-transport <trip-id> <item-id> \
  --duration-minutes 25 --yes
funliday trips use-auto-transport <trip-id> <item-id> --yes
funliday trips calculate-transport <trip-id> <item-id> --yes
funliday trips set-place-note <trip-id> <item-id> \
  --note "Remember to reserve" --yes
funliday trips show-place-note <trip-id> <item-id>
funliday trips delete-place <trip-id> <item-id> --yes
```

`trips list` returns both the itinerary `_id` and the `containerId`; deletion
uses the latter.

## MCP server

The stdio MCP server is read-only by default:

```bash
funliday mcp
```

It exposes trip reads and city/place search. To advertise trip and itinerary
write tools, the host must start `funliday mcp --enable-write`; every write call
must additionally include `confirm: true`. Write tools cover trip creation and
deletion, searched and custom places, fixed start/stay times, custom travel
durations, notes, and place deletion.

Transportation fields belong to the departure place: an `<item-id>` describes
the segment from that place to the next place on the same day. Automatic route
calculation requires the POI Bank session captured by a recent `funliday login`.

Login opens Funliday's official page in an isolated Chrome profile. Passwords and
social-login credentials stay on that page. The resulting session token is stored
in the operating system's standard config directory as a mode-`0600` file on Unix;
the project does not require macOS Keychain.

Do not commit the local config directory, browser profile, tokens, captured
requests, or personal itinerary exports.
