# edx-kicad

Converts a KiCad board (`.kicad_pcb`, KiCad 6 and later) to an EDX document, with no KiCad running. The project file
beside the board (`.kicad_pro`) supplies the net classes. It is one of the free utilities that come with the EDX
specification and libraries.

```sh
edx from-kicad board.kicad_pcb -o board.edx.json          # design, rules and the author's layout
edx from-kicad --no-layout --fixed locked board.kicad_pcb # the design and rules only
```

```rust
let board = std::fs::read_to_string("board.kicad_pcb")?;
let project = std::fs::read_to_string("board.kicad_pro").ok();
let converted = edx_kicad::convert(&board, project.as_deref(), &edx_kicad::Options::default())?;
std::fs::write("board.edx.json", edx_format::to_string(&converted.document))?;
for note in converted.notes {
    eprintln!("{note}"); // what was left out or approximated
}
```

## What goes where

| KiCad | EDX |
| --- | --- |
| Edge.Cuts lines, arcs, circles, rectangles, polygons and curves, on the board or in footprints | `design.board.outline`; closed loops inside others are cut-outs. A board with no closed loop has no outline, and a note says so |
| Copper layers `F.Cu`, `In1.Cu` … `B.Cu` | `stackup` layers `top`, `In1` … `bottom` |
| The board's physical stack-up | `copper_thickness`, `dielectric` (sublayers merged, permittivity averaged by thickness) and `mask` |
| An inner layer at least half covered by one net's zones, with no other net's tracks | a `plane` layer with that `plane_net` |
| A footprint | a package (pads and courtyard as seen from the top) and a part; footprints of the same geometry share a package |
| Pads: rect, circle, oval, roundrect | the same shapes |
| Pads: trapezoid, chamfered, custom | `polygon` (a chamfered pad's other corners are square; a custom pad is its anchor and filled primitives joined) |
| Courtyard (`F.CrtYd` or `B.CrtYd`, the part's own side) | `courtyard.top`; a footprint with none is converted without one (readers then take the nominal courtyard round its pads) |
| Net classes and their nets (`.kicad_pro`) | `rules.classes`; `Default` is `default` |
| Minimum track width, copper to edge, hole to hole | `TrackWidth.min`, `rules.board` |
| Copper zones | `design.pours`, ranked by KiCad's priority, then by the order they are written in |
| Keep-out zones and rule areas | `design.regions` |
| Where each footprint is, its tracks, arcs and vias | `layout` (unless `--no-layout`), with the hashes of the design and rules it was made from |

Each part is `fixed` where the author put it or `free` for a layout engine to place (`--fixed`):

- `edge` (the default): parts the author locked, mounting holes and fiducials, and parts whose courtyard comes within 1
  mm of the outline or crosses it, such as connectors;
- `locked`: only the parts the author locked;
- `all` or `none`.

A part on the bottom is turned half a turn more in EDX than in KiCad: EDX mirrors a package left to right, KiCad top to
bottom.

Left out: silkscreen and other graphics, text, footprints with no pads, filled zone copper, teardrops, nets that reach
no pad or that KiCad names `unconnected-(...)`, and copper on no net. KiCad 5 boards are refused: open and
save them in KiCad 6 or later first.

## Licence

MIT OR Apache-2.0. The boards in `tests/data` were written for these tests and are CC0, like the conformance fixtures.
