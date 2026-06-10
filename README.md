# rust_sim_city

A small SimCity-style city builder written in Rust with
[macroquad](https://github.com/not-fl3/macroquad). Draw roads, paint
residential/commercial/industrial zones, and watch the city grow — or decay —
driven by an RCI demand feedback loop.

## Run

```sh
cargo run --release
```

Requires stable Rust. On Linux you need the usual X11/OpenGL runtime libraries
(present on any desktop system; on Debian/Ubuntu:
`libx11-6 libxi6 libgl1`).

## Controls

| Input              | Action                                  |
| ------------------ | --------------------------------------- |
| `W A S D` / arrows | Pan camera                              |
| Mouse wheel        | Zoom (anchored at cursor)               |
| `1` or button      | Road tool ($10/tile)                    |
| `2` or button      | Bulldozer ($1/tile)                     |
| `3` / `4` / `5`    | Zone residential / commercial / industrial ($5/tile) |
| Left click / drag  | Apply current tool                      |
| `Space`            | Pause/resume the simulation             |

## How it plays

- Zones only develop when a road is within 3 tiles. Zones with no road access
  stay as faint colored dirt; if you bulldoze their road, developed buildings
  gradually abandon back to dirt.
- The RCI meter (top right) shows demand for each zone type, -1..1:
  - **R** rises with available jobs, falls when housing outstrips employment.
  - **C** rises with population, falls as commercial supply saturates.
  - **I** rises with population and commerce, falls as industry saturates.
- Each simulation tick (0.5 s), connected zones with positive demand have a
  chance to level up (0–5, rendered as more/brighter buildings); strongly
  negative demand makes them decay.
- Developed zones pay tax each tick; roads cost a little upkeep. Zone all
  three types near roads to keep money and demand flowing.

## Implemented vs. stubbed

Implemented: 64×64 grid, pan/zoom camera, road/bulldoze/zone tools with drag
painting (gap-free line interpolation), funds with build costs and tax income,
the RCI demand loop, road-proximity gating, growth/abandonment, level-based
building rendering, HUD with funds/population/jobs/tool buttons/RCI meter,
pause, and unit tests covering the simulation (`cargo test`).

Not implemented (out of scope): road *network* pathfinding (access is
proximity-based, not graph-connected), power/water utilities, land value,
traffic, save/load, sound, and any real art — everything is colored rectangles
by design.
