# zingolib-tui

A terminal wallet for Zcash mainnet, built on [zingolib](https://github.com/zingolabs/zingolib).

## Features

- Create a new wallet (24-word BIP-39 seed), restore from a seed phrase, or watch-only from a UFVK
- Continuous shielded sync with progress strip / chain map, pause/resume, and rescan
- Send ZEC to unified, sapling or transparent addresses, with memos and fee preview
- Receive addresses with QR codes (unified and transparent)
- Transaction history with inputs/outputs, memos, wallet events, and per-transaction detail
- Per-pool balances: ironwood, orchard, sapling, transparent
- 16 colour themes with live preview, truecolor / 256-colour support
- Copy addresses and txids to the clipboard over OSC 52 (works over SSH)
- Shielded voting (`b`): fetch the active round, fill the ballot per proposal, cast, and watch the lifecycle log. Normal mode resolves the production vote config; `--voting-staging` opts into the staging network, and `ZINGO_VOTING_STATIC_CONFIG_SOURCE=<url>` points either mode at any source
- `--fake` demo mode: explore every screen with made-up data, no wallet or network — including the full voting lifecycle

## Layout

- `crates/core` — reusable wallet model and zingolib backend (state, reducer, sync lifecycle)
- `crates/tui` — the terminal frontend (ratatui views, input, clipboard, QR)

## Running

```
cargo run --release -- --help
```

Depends on the `dev` branch of zingolabs/zingolib.

## Voting build

The voting screen builds against the shielded-voting branches: `zingolib` and
its stack from `feat/lightclient-voting-api`, and the `zcash_voting` SDK from
`feat/lrz-librustzcash-0-31-port` (git-patched in the root `Cargo.toml` until
releases land). The default voting source is the production anchor pinned in
zingolib (`voting.valargroup.dev/pins/prod/…`); `--voting-staging` opts into
the stage network, and `ZINGO_VOTING_STATIC_CONFIG_SOURCE=<url>` points either
mode at any source. A real cast needs a seed-backed, synced mainnet wallet.
Swap the branch deps for rev pins (or main) once the voting branches land.
