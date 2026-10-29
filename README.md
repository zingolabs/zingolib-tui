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
- `--fake` demo mode: explore every screen with made-up data, no wallet or network

## Layout

- `crates/core` — reusable wallet model and zingolib backend (state, reducer, sync lifecycle)
- `crates/tui` — the terminal frontend (ratatui views, input, clipboard, QR)

## Running

```
cargo run --release -- --help
```

Depends on the `dev` branch of zingolabs/zingolib.
