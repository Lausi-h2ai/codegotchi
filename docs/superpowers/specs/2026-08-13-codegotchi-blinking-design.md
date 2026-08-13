# CodeGotchi Blinking Design

## Goal

Give an awake CodeGotchi a natural single blink at a newly randomized interval
between five and ten seconds. Sleeping pets keep their eyes closed through the
existing sleeping presentation and never run the awake blink cycle.

## Architecture

Blinking remains presentation-only. A focused React hook owns the blink timer
and exposes whether the eyes are currently closed. `App` enables that hook
unless the motion layer's semantic mode is `napping`, and reflects the result
as a data attribute on the existing pet element. CSS changes the existing eye
shapes while that attribute is active.

The authoritative Rust simulation, WebSocket protocol, persistence, and pet
motion/choreography controller do not change. Blinking does not create a new
motion action because it must compose with every awake pose and action.

## Behavior

- After an awake pet is presented, the first blink begins after a random delay
  in the inclusive range from 5,000 to 10,000 milliseconds.
- A blink is one close-and-open cycle. Both eyes close together for about 120
  milliseconds, then reopen.
- After reopening, the hook chooses a fresh random 5–10 second delay for the
  next blink.
- Entering the `napping` semantic mode cancels any pending awake blink and
  immediately clears an in-progress blink.
- Leaving `napping` starts a fresh randomized wait; it does not reuse elapsed
  time from before the nap.
- Unmounting cancels all blink timers.
- All awake semantic modes may blink, including free time, desk work,
  thinking, critical, success, and failure presentations.

## Components

### Blink hook

The hook accepts an `enabled` boolean and returns a boolean closed-eye state.
It uses injectable randomness only where needed for deterministic unit tests,
while production uses `Math.random`. Delay selection clamps the random input
to the supported unit interval so the scheduled delay cannot escape the
5–10-second bounds.

### App adapter

`App` enables blinking when `motionState.semanticMode !== "napping"` and adds
`data-blinking="true"` only during the brief closed-eye phase. This preserves
the motion layer as the source of truth for presentation-level sleeping.

### Eye styling

The blink selector flattens both existing `.pet-eye` elements into short,
rounded horizontal lines while preserving their placement. It is more
specific than awake pose eye styling but does not override the existing
napping selector.

## Testing

Fake-timer hook tests cover the minimum and maximum randomized wait, the
single close/open cycle, selection of a fresh delay, cancellation and reset
when disabled, restart when re-enabled, and timer cleanup on unmount. An App
adapter test confirms that awake state exposes the blink attribute during a
cycle and napping state never does. Existing web tests, lint, formatting, and
the production build remain green.

## Out of Scope

Double blinks, winks, backend blink state, persisted blink timing, sound,
eyelid artwork, and changes to sleeping visuals are not part of this feature.
