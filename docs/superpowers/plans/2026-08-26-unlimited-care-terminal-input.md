# Unlimited care items and upper-terminal input

## Goal

Keep the needs meters authoritative and draining while making every care item
reusable forever. The terminal room must show each care-item name without a
quantity, and the upper Codex pane must support scrolling while retaining
Linux terminal-native Shift selection/copy/paste behavior.

## Implementation steps

1. Add an explicit unlimited inventory representation in the domain without
   changing the persisted snapshot shape. Make product-created and restored
   pets use it, keep care effects and need progression unchanged, and make
   repeated feeding leave the inventory unchanged.
2. Update the web and terminal room projections to render all known care-item
   names without counts. Keep debug restock as a compatible no-op/reset to the
   unlimited inventory and update domain, CLI, and web tests that currently
   assert depletion or numeric labels.
3. Expose Codex scrollback movement through the terminal screen seam. Route
   upper-pane wheel events to that local scrollback when the child has not
   negotiated mouse tracking, while preserving room capture and child input
   routing for negotiated protocols.
4. Add focused regression tests for repeated care, persisted-state
   normalization, name-only rendering, upper-pane scrolling, and native paste
   forwarding. Run the focused suite and then the full workspace checks.
5. Exercise the production Linux xterm path and visually inspect the changed
   Full, Compact, and Minimal room layouts and the supported terminal themes.
   Record the exact artifacts and any coverage gaps in the verification
   ledger, without touching the existing audit captures.

## Verification

- `cargo test -p codegotchi-domain`
- `cargo test -p codegotchi-cli`
- `npm test -- --run` (from `web/`, if the workspace scripts require it)
- Real `codegotchi run --ui terminal -- codex` under xterm with wheel and
  Shift selection/paste probes at supported room sizes/themes.
