# Grounded Pet Roaming Design

## Problem

Pixel's free-time choreography treats every room waypoint as the position of
the pet's feet. The room floor begins at 70% of the illustration height, but
the current waypoint set includes Y coordinates from 27% to 61%. Traveling to
those points therefore moves the whole pet through the wall area, making Pixel
look as though she is flying instead of walking around the room.

## Desired behavior

Pixel remains grounded throughout free-time travel and actions. She continues
to visit the window and shelf, but observes them from nearby floor positions
rather than moving onto the wall. The existing walking bounce, rolls, pauses,
and interaction-specific actions remain cosmetic and unchanged.

## Design

The choreography waypoint data is the source of truth for room-relative pet
placement, so the fix belongs in `web/src/choreography.ts`. Every safe roaming
waypoint will use a Y coordinate in the visible floor band below the room's 70%
floor boundary. X coordinates may be adjusted slightly so Pixel's body remains
clear of the feed, poop, shovel, and trash areas.

The `window` and `shelf` regions and their `watch_window` and `inspect_shelf`
interactions remain intact. Their coordinates become grounded observation
positions. React continues to translate normalized waypoint coordinates into
`--pet-x` and `--pet-y`, and CSS continues to anchor the pet by her feet with
`translate(-50%, -100%)`. No backend state, protocol, care operation, or motion
timer behavior changes.

## Testing

A focused unit regression test will assert that every safe free-time waypoint
places the pet's feet within the floor band while remaining outside forbidden
care-control zones. This test would fail if a future waypoint is placed back on
the wall.

A production-browser assertion will compare the pet's rendered bottom-center
anchor with the room's computed floor boundary during free-time movement. It
will observe multiple regions, including the window and shelf interactions,
and require every observed position to remain grounded. Existing tests continue
to cover rolls, semantic interruption, reduced motion, and care-control access.

## Scope

This change is limited to free-time waypoint placement and the regression
coverage needed to protect it. Semantic desk, thinking, critical, outcome, and
hammock positions are not part of this repair.
