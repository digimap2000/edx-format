# EDX: Electronics Design eXchange, version 1.0

**Status:** working draft, not yet stable. Breaking changes are still possible before 1.0 is released.

**Licence:** Creative Commons Attribution 4.0 International.

## 1. Introduction

EDX is a tool-neutral exchange format for printed circuit board design data. It carries three things between a
*design side* (a schematic tool, a generator script, an AI agent) and a *layout engine* (a placer and router, human
or automatic):

- **what the board is**: outline, stack-up, packages, parts, nets, keep-out regions and pours;
- **what it must achieve**: relations between pads, rules for each kind of relation, and explicit, checkable
  constraints;
- **what the engine did**: placement, copper and a report on every constraint.

EDX describes intent and geometry in plain terms. It does not describe how a layout engine works, which algorithms
it runs or how it stores its history. Conversion to and from the native formats of EDA tools is the job of
adapters, outside this specification.

### 1.1 Principles

1. **Abstract.** No tool-specific types or names. Geometry, connectivity, intent and rules are stated in plain terms.
2. **Intent, rules and constraints are separate layers.**
    - *Relations* say what relates to what, pad to pad, with no values ("C5.1 decouples U1.9").
    - *Rules* give each kind of relation its values and style ("decouples: track at most 2.5 mm").
    - *Constraints* are explicit, checkable statements, derived from relations and rules or authored directly
      ("track C5.1 to U1.9 at most 2.5 mm, pad centre to pad centre").
3. **Constraints bound; objectives choose.** Every requirement that must hold is an explicit constraint with a
   source. How an engine chooses among the layouts that satisfy them (wirelength, congestion and so on) is outside
   this specification.
4. **Order-independent.** The meaning of a file never depends on the order in which parts, nets, pads or other
   records appear.
5. **Measured the same way everywhere.** Every constraint states how it is measured (section 9.3), so the design
   side, the engine and any checker agree.
6. **Versioned.** Every file states its EDX version. Within a major version, changes are backwards compatible
   (section 12).

### 1.2 Conformance

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, RECOMMENDED, MAY and OPTIONAL in this
document are to be interpreted as described in BCP 14 (RFC 2119 and RFC 8174) when, and only when, they appear in
all capitals.

This specification defines two roles:

- An **EDX reader** reads a bundle or result. A conforming reader MUST apply every check in section 14 that this
  specification marks as an error, and MUST refuse input that fails one.
- An **EDX writer** writes a bundle or result. A conforming writer MUST produce files that pass every check in
  section 14, and SHOULD write canonical form (section 13).

The JSON Schemas published with this specification are normative for structure. Where a schema and this text
disagree, this text governs and the schema is a defect to be corrected.

### 1.3 Terminology

- **Bundle:** the set of input files that describe one design (section 4).
- **Result:** the output of a layout engine for a bundle: a layout file and a report (section 11).
- **Package:** reusable footprint geometry, independent of any EDA library.
- **Part:** one instance of a package on the board, identified by its reference (`U1`, `C5`).
- **Pad reference:** a part reference and a pad number together, naming one pad on the board.
- **Registry:** a list of names this specification defines and that may grow in a minor version (section 12.2).

## 2. Conventions

### 2.1 Units and precision

- Lengths are in millimetres. Angles are in degrees.
- A reader MUST round every length to the nearest nanometre (0.000001 mm), with ties rounded away from zero, and
  MUST compare lengths only after rounding.
- A reader MUST normalise every angle to the range [0, 360). Angles are compared to 0.000001 degrees.
- Numbers are written in decimal notation. A writer MUST NOT write `NaN`, infinities or exponent notation.

### 2.2 Axes and angles

- The origin is at the top-left of the board outline's bounding box. x increases to the right and y increases
  downwards, as seen from the top.
- Positive angles turn counter-clockwise as seen from the top.

### 2.3 Sides and mirroring

- Every package is defined as seen from the top.
- A part on the `bottom` side is its package mirrored left to right (x becomes -x, as seen from the top), then turned
  by its angle, counter-clockwise as seen from the top.
- Mirroring a part to the bottom maps its `top` copper to `bottom` and reverses the order of inner layers.

### 2.4 Points and polygons

- A **point** is an array of two numbers, `[x, y]`.
- A **ring** is an array of three or more points. It is implicitly closed: the last point MUST NOT repeat the
  first. Consecutive points MUST be distinct, and a ring MUST NOT intersect itself.
- A **polygon** is an object `{ "outer": ring, "holes": [ring, ...] }`. `holes` is OPTIONAL and defaults to none.
  Each hole MUST lie inside the outer ring, and holes MUST NOT overlap each other.
- Winding direction carries no meaning. A reader MUST accept either direction. In canonical form an outer ring runs
  counter-clockwise and a hole clockwise, as seen from the top.
- A field that accepts **polygons** takes an array of one or more polygons, which together cover their union.
- Version 1.0 has no curved edges in polygons. A writer approximates arcs with straight segments.

### 2.5 Names and references

- A **name** (a part reference, pad number, net, class, layer, package id, region or block name) is a non-empty
  Unicode string in NFC form with no leading or trailing white space and no control characters. Names are
  case-sensitive.
- In JSON, a pad reference is always written as an object, `{ "part": "C5", "pad": "1" }`. Pad numbers are names, not
  numbers: `"1"`, `"A12"` and `"EP"` are all valid.
- The **display form** of a pad reference is `part.pad` (`C5.1`). It is for people and messages. Because part
  references and pad numbers may themselves contain `.`, a reader MUST NOT parse the display form.
- An **id** built from names (section 6.1) escapes each name before joining it: `%` becomes `%25`, `.` becomes
  `%2E` and `:` becomes `%3A`.

### 2.6 Files

- Every file is UTF-8 without a byte-order mark.
- Generated files are JSON (RFC 8259). Files written by hand are TOML 1.0. Both are validated against the same JSON
  Schemas through their shared data model.
- Every object MAY carry an `extras` object for application-specific data (section 12.4).

## 3. Overview of the files

| File | Written by | Format | Contents | Section |
| --- | --- | --- | --- | --- |
| `layout.toml` | hand or tool | TOML | the manifest: version, files, extensions | 4 |
| `design.json` | design side, generated | JSON | board, packages, parts, nets, regions, pours, relations | 5, 6 |
| `rules.toml` | hand | TOML | electrical rules per net class; layout rules per relation kind | 8 |
| `constraints.json` | derived or hand | JSON | explicit constraints | 9 |
| `layout.json` | layout engine | JSON | the result: placement, copper, engine regions and pours, groups | 11.1 |
| `report.json` | layout engine | JSON | the result: status of every constraint and the gates | 11.2 |

## 4. The bundle

A bundle is a directory holding a manifest named `layout.toml` and the files it lists.

```toml
edx = "1.0"

[design]
file = "design.json"

[rules]
file = "rules.toml"

[constraints]
file = "constraints.json"

[extensions]
used = []
required = []
```

- `edx` is REQUIRED: the EDX version the bundle is written for (section 12.1).
- `design` and `rules` are REQUIRED. `constraints` is OPTIONAL; a bundle without it has only the constraints derived
  from its relations and rules (section 9.5).
- `file` paths are relative to the manifest, use `/` as separator and MUST NOT leave the bundle directory.
- Each listed file MUST itself carry an `edx` version with the same major version as the manifest.
- `extensions` is OPTIONAL (section 12.3).
- A bundle MAY carry other files, such as a template for an EDA tool. Readers ignore files the manifest does not
  list, unless an extension says otherwise.

## 5. Design description (`design.json`)

```json
{
  "edx": "1.0",
  "board": { "outline": [], "stackup": [], "sides": {} },
  "packages": [],
  "parts": [],
  "nets": [],
  "regions": [],
  "pours": [],
  "relations": []
}
```

`edx`, `board`, `packages`, `parts` and `nets` are REQUIRED. The other arrays are OPTIONAL and default to empty.

### 5.1 Board

- `outline` (polygons, REQUIRED): the board's shape. Holes are cut-outs.
- `stackup` (REQUIRED): the copper layers in order, top first. Each is
  `{ "name", "role": "signal" | "plane", "routable": bool, "plane_net"? }`.
    - There MUST be at least two layers. The first MUST be named `top` and the last `bottom`.
    - A `plane` layer MUST name its `plane_net`. That net fills the layer except where pours of other nets on it
      take precedence (section 5.6), as an island of `+1V8` inside a `+3V3` plane.
    - `routable: false` forbids tracks on the layer.
- `sides` (OPTIONAL): which sides may carry parts.
  `{ "default": ["top", "bottom"], "classes": { "<package class>": ["bottom"] } }`. When absent, both sides are
  allowed for every part.

### 5.2 Packages

A package is footprint geometry, referenced by its `id` and independent of any EDA library.

- `id` (name, REQUIRED, unique among packages).
- `class` (REQUIRED): from the registry `ic | passive | connector | mechanical | other`. Rules use it, for example to
  decide what counts as a fine-pitch IC.
- `pads` (REQUIRED): each pad has
    - `number` (name, REQUIRED). Two pads in one package MAY share a number; they are then the same electrical pad.
    - `kind`: `smd | th | npth | edge`.
    - `shape`: `rect | roundrect | circle | oval | polygon`. `roundrect` adds `corner_radius`; `polygon` adds
      `polygon` (one polygon, relative to the pad's position) instead of `size`.
    - `size` `[width, height]`, `position` (point), `rotation` (angle).
    - `layers`: `top | bottom | through`.
    - `drill` (REQUIRED for `th` and `npth`): `{ "diameter" }` for a round hole or `{ "slot": [width, height] }`.
    - `zone_connection` (OPTIONAL, overrides the pad's net class): `{ "mode": "solid" | "thermal" | "none",
      "spoke_width"?, "gap"? }`.
- `courtyard` (REQUIRED): `{ "top": polygons, "bottom"?: polygons }`, on the package's own side, with the other
  side OPTIONAL.
- `body` (OPTIONAL): `{ "outline": polygons, "height" }`, for 3D and keep-away checks.

### 5.3 Parts

Each part is `{ "ref", "package", "value"?, "side", "placement", "attributes"? }`.

- `ref` (name, REQUIRED, unique among parts).
- `package` (REQUIRED) MUST be the `id` of a package in the same file.
- `side`: `top | bottom`.
- `placement` is one of:
    - `{ "kind": "free" }`: the part has no position until a layout engine gives it one. An engine MUST report a
      free part it could not place as unplaced and MUST NOT give it an arbitrary position.
    - `{ "kind": "fixed", "x", "y", "angle" }`: the position is mandated (connectors, fixings, marks). The part's
      origin is the package origin.
- `attributes` (OPTIONAL): a map of names to strings, carried but not interpreted by this specification.

### 5.4 Nets

Each net is `{ "name", "pads": [pad reference, ...], "class"? }`.

- `name` is unique among nets. Every pad reference MUST name an existing part and a pad number in its package.
- A pad belongs to at most one net. A pad in no net is unconnected.
- `class` names a net class defined in `rules.toml` (section 8.1). When absent, the net is in class `default`.

### 5.5 Regions

A region is a named area that excludes things.

- `name` (REQUIRED, unique among regions), `polygons` (REQUIRED).
- `layers`: an array of layer names, or `"all"`.
- `excludes`: one or more of `parts | pads | tracks | vias | pours`.
- `except_nets` (OPTIONAL): nets whose copper the region allows.
- `owner` (OPTIONAL): `{ "part": ref }` or `{ "block": name }`. An owned region is given in its owner's coordinates
  and moves with it. Without an owner it belongs to the board.

### 5.6 Pours

A pour is copper fill as design intent: `{ "net", "layers", "polygons", "priority", "connection", "clearance" }`.

- On a signal layer, a pour is, for example, a ground fill.
- On a plane layer, a pour of another net is an island in the plane. Its `priority` MUST be higher than the plane's.
- A plane layer's own net (section 5.1) has priority 0 and fills what the layer's pours leave.
- Where two pours overlap, the higher `priority` wins. Two overlapping pours of different nets with the same priority
  are an error.
- `connection`: as `zone_connection` in section 5.2.

## 6. Relations

A relation records design intent between two pads, with no values:

```json
{ "kind": "decouples", "subject": { "part": "C5", "pad": "1" }, "object": { "part": "U1", "pad": "9" } }
```

### 6.1 Identity

- A relation's id is `kind:subject:object`, each pad reference in display form and each name escaped as in section
  2.5: `decouples:C5.1:U1.9`.
- A writer MAY state the id; a reader MUST derive it and MUST refuse a stated id that differs.
- Two relations with the same id are an error.
- A part MAY have several relations (a crystal: `X1.1 clocks U1.12` and `X1.2 clocks U1.13`).
- `source` (OPTIONAL) records where the relation came from, as free text or a URI.

### 6.2 Relation kinds

The kind list is a registry. Registered kinds:

| Kind | Subject | Object | Meaning |
| --- | --- | --- | --- |
| `decouples` | high-frequency capacitor pad | IC supply pin | the capacitor serves that pin |
| `backs` | bulk capacitor pad | IC supply pin | backs the high-frequency capacitors on that pin's supply |
| `filters` | filter capacitor pad | IC supply pin | the capacitor of the filter on that pin's own rail (the C of an RC or LC filter) |
| `feeds` | series filter part pad | the pin or capacitor pad it supplies | a ferrite bead, inductor or resistor feeding a filtered rail |
| `clocks` | crystal pad | IC oscillator pin | crystal leg |
| `loads` | load capacitor pad | crystal pad | crystal load |
| `near` | any pad | any pad | keep close |

A relation of a kind that is neither registered nor provided by a supported extension is an error. A relation of a
known kind with no rule in `rules.toml` (section 8.2) is an error.

## 7. Blocks

A block is a set of parts, with their copper and regions, frozen relative to each other and handled as one rigid
body: it moves and turns as one, and does not flip in version 1.0.

- The design side requests a block with a `block` constraint (section 9.2). A layout engine fulfils the request and
  reports the block in its result (section 11.1). An engine MAY also form blocks of its own.
- A block's **courtyard** is the union of its members' courtyards and its own regions.
- A block's **pins** are the pads whose nets leave the block.
- Constraints between a block's members still apply and are re-measured in every report.
- Allowed turns are given by an `orientation` constraint on the block.
- Blocks MAY nest.

## 8. Rules (`rules.toml`)

```toml
edx = "1.0"

[board]
copper_to_edge = 0.3
hole_to_hole = 0.25

[classes.default]
clearance = 0.15
track_width = { min = 0.15, preferred = 0.2 }
via = { pad = 0.6, drill = 0.3 }

[classes.Supply]
clearance = 0.2
track_width = { min = 0.3, preferred = 0.5 }
layers.bottom = { track_width = { min = 0.4, preferred = 0.6 } }

[relations.decouples]
max_track = 2.5
square_on_pin = true
via = "beside_pad"
adjacent_pins_via = "beside_pad"

[relations.backs]
max_track = 6.0
```

### 8.1 Electrical rules

- `[board]` holds board-wide minimums: `copper_to_edge` and `hole_to_hole`.
- `[classes.<name>]` defines a net class: `clearance`, `track_width` (`min` and `preferred`) and `via` (`pad` and
  `drill`), with per-layer overrides under `layers.<layer name>`.
- Class `default` MUST be defined. Every class a net names MUST be defined.

### 8.2 Layout rules

`[relations.<kind>]` gives the values and style for each relation kind used in the design. The keys each kind accepts
are part of that kind's registry entry:

| Kind | Keys |
| --- | --- |
| `decouples` | `max_track`, `square_on_pin` (bool), `via`, `adjacent_pins_via` (each a `via_policy` value, section 9.2) |
| `backs` | `max_track` |
| `filters` | `max_track` |
| `feeds` | `max_track` |
| `clocks` | `same_layer` (bool), `no_vias` (bool), `matched_tolerance`, `quiet_margin` |
| `loads` | `max_track` |
| `near` | `max_distance`, `measure` |

An unknown key is an error.

## 9. Constraints (`constraints.json`)

```json
{
  "edx": "1.0",
  "constraints": [
    {
      "id": "max_track:C5.1:U1.9",
      "type": "max_track",
      "operands": [{ "part": "C5", "pad": "1" }, { "part": "U1", "pad": "9" }],
      "value": 2.5,
      "hard": true,
      "origin": "derived",
      "source": { "relation": "decouples:C5.1:U1.9", "rule": "decouples.max_track" }
    }
  ]
}
```

### 9.1 Common fields

- `id` (REQUIRED, unique): stable across versions of the design. Reports refer to constraints by id.
- `type` (REQUIRED): from the registry in section 9.2.
- `operands`, `value`, `measure`: as the type defines.
- `hard` (default `true`). A soft constraint (`false`) carries `cost`, in millimetre-equivalents per unit of
  violation, on the same scale as wirelength. Anything that must hold is hard.
- `origin`: `derived` (from a relation and a rule) or `authored`.
- `source` (REQUIRED): `{ "relation", "rule" }` for a derived constraint, or `{ "document" }` (free text or a URI)
  for an authored one.

### 9.2 Constraint types

The type list is a registry. Registered types:

| Type | Operands | Value | Measured |
| --- | --- | --- | --- |
| `max_track` | pad, pad | length | pad centre to pad centre. Once joined: along the copper from where each track meets each pad, on to the pad centres, vias counting zero. Before: the straight line between centres. |
| `max_distance` | pad or part, pad or part | length | straight line. `measure: "centre" \| "edge"`, REQUIRED. |
| `max_length` | net or path | length | routed length |
| `matched` | nets | tolerance | routed lengths equal within the tolerance |
| `square_on_pin` | part, pin | `tolerance`? | the part's pad on the pin's line, facing it. Soft by default, costed by sideways offset; hard when a tolerance is given, the offset held within it. |
| `via_policy` | relation group | `in_link \| beside_pad \| none` | `in_link`: room for a via between the pin's tip and the pad, the via there. `beside_pad`: beside the pad, pointing away from the link. `none`: the link has no via. |
| `same_layer` | nets or path | none | all copper on one layer |
| `no_vias` | nets or path | none | no vias |
| `quiet` | group | margin | other nets' copper kept the margin from the group on every layer |
| `keepout` | region | none | as section 5.5 |
| `fixed` | part | `{x, y, angle}` | position and angle as given |
| `side` | part | `top \| bottom` | the part on that side |
| `orientation` | part or block | set of angles | the allowed angles |
| `block` | parts | `{name}` | the parts and their copper form a rigid block (section 7) |

Operands are written as objects: `{ "part", "pad" }` for a pad, `{ "part" }` for a part, `{ "net" }`,
`{ "block" }`, `{ "region" }`, and `{ "relations": [id, ...] }` for a relation group. A path is an ordered array of
pad references.

### 9.3 Measures

Every constraint type above states how it is measured. A type whose measure has options (`max_distance`) MUST state
`measure`. Checkers MUST measure as stated, so that the design side, the engine and the checker agree on whether a
constraint is met.

### 9.4 Status

A constraint is `met`, `broken`, or `not_yet_measurable` (for example, a `matched` group not yet routed). Reports use
these values (section 11.2).

### 9.5 Derivation and precedence

- Constraints MAY be derived from relations and rules, by the design side or by a layout engine.
- A derived constraint's id is `type:` followed by its operands in display form, joined by `:` and escaped as in
  section 2.5.
- An authored constraint wins over a derived constraint with the same id.
- Two authored constraints that contradict each other are an error. They contradict when no layout could meet both:
  for example, two `fixed` constraints on one part with different positions, or `side` constraints naming different
  sides.

## 10. Measurement of placement

- A part's position is the position of its package origin. Its angle is the turn of the package, after mirroring for
  the bottom side (section 2.3).
- Pad positions on the board are derived from the part's position, side and angle, and the pad's position and
  rotation in its package.

## 11. Results

A layout engine returns a result for a bundle as two files. Each records the hashes of the bundle files it was made
from (section 13.3), so a result can be matched to its inputs.

```json
{ "edx": "1.0", "inputs": { "design.json": "sha256:...", "rules.toml": "sha256:...", "constraints.json": "sha256:..." } }
```

### 11.1 Layout (`layout.json`)

- `placement`: per part `{ "ref", "placed", "x"?, "y"?, "angle"?, "side"?, "locked" }`. An unplaced part has
  `placed: false` and no coordinates.
- `copper`:
    - `tracks`: `{ "net", "layer", "width", "start", "end", "locked" }`;
    - `arcs`: `{ "net", "layer", "width", "start", "mid", "end", "locked" }`;
    - `vias`: `{ "net", "at", "pad", "drill", "layers": [from, to], "locked" }`.
- `regions` and `pours`: those the engine added, in the form of sections 5.5 and 5.6, kept apart from the design's.
- `groups`: `blocks` (`{ "name", "parts", "angles"? }`) and quiet groups (`{ "name", "nets", "margin" }`).

### 11.2 Report (`report.json`)

- `constraints`: per constraint id, `{ "id", "status", "measured"?, "limit"?, "measure"?, "hard", "cost"?,
  "where"? }`, where `where` gives the parts, nets or pads involved and coordinates.
- `unplaced`: the references of parts that are not placed.
- `overlaps`: pairs of parts or blocks whose courtyards overlap.
- `unrouted`: per net, the pad references not yet connected.
- `clearance`: clearance violations found by the engine's own check.
- `gates`: `{ "constraints_met", "all_placed", "all_routed", "clearance_clean" }`, each a boolean.

Every broken hard constraint MUST be listed. Unplaced parts are reported separately from overlaps.

## 12. Versioning and extensions

### 12.1 Versions

- An EDX version is `MAJOR.MINOR`. Every file states it in a top-level `edx` field.
- A minor version only adds: optional fields, registry entries, error codes. A reader for version `M.n` MUST read a
  file of version `M.m` for any `m`. Where `m > n`, it MUST ignore fields it does not know, warn, and still apply
  every check it knows.
- A major version MAY make incompatible changes. A reader MUST refuse a file with a major version it does not
  support.
- Editorial corrections to this text that change no requirement are published as `MAJOR.MINOR.PATCH` of the
  document only. Files never state a patch number.

### 12.2 Registries

Package classes (5.2), relation kinds (6.2), layout rule keys (8.2), constraint types (9.2) and error codes (14) are
registries. Registered names are bare words. A minor version MAY add to a registry and MUST NOT remove from or change
an entry.

### 12.3 Extensions

- An extension adds registry entries or fields under a namespaced name, `<namespace>.<name>`, such as
  `acme.shield_can`. The namespace is a name its author controls, such as a company or project name.
- The manifest lists every extension the bundle uses in `extensions.used`, and those a reader must understand to read
  it correctly in `extensions.required`.
- A reader MUST refuse a bundle that requires an extension it does not support. A reader MAY ignore an extension that
  is used but not required, and SHOULD warn when it does.
- An extension MAY be proposed for registration, which makes its names bare in a later minor version.

### 12.4 Extras

Every object MAY carry an `extras` object. Readers MUST preserve `extras` when they rewrite a file and MUST NOT
change its meaning. Tools use it for data that is theirs alone. Data other tools need belongs in an extension.

## 13. Canonical form and hashing

### 13.1 Canonical JSON

A canonical JSON file:

- has object keys in the order of its schema;
- sorts arrays whose order has no meaning (section 1.1, principle 4) by their key: `ref` for parts, `id` for
  packages and constraints, `name` for nets, regions and classes, the relation id for relations, and pad references
  by part then pad;
- writes lengths to at most six decimals and angles to at most six decimals, with trailing zeros and a trailing
  decimal point removed;
- indents by two spaces, uses LF line endings and ends with a single newline.

### 13.2 Canonical TOML

Canonical TOML follows the same key order, sort order and number form, with tables in schema order.

### 13.3 Hashing

The hash of a file is `sha256:` followed by the lowercase hex SHA-256 digest of its canonical form. Two files with
the same meaning have the same hash, whatever their formatting or order.

## 14. Validation

### 14.1 Layers

A reader checks a bundle in three layers, in order:

1. **Parse:** syntax of JSON or TOML, and UTF-8.
2. **Schema:** structure, types, required fields and allowed values, against the published JSON Schemas.
3. **Semantics:** every rule in this text that a schema cannot express.

### 14.2 Diagnostics

Every problem a reader reports carries a code, a severity (`error` or `warning`), the file, a JSON Pointer
(RFC 6901) to the value, and a message. A reader SHOULD add line and column.

### 14.3 Error codes

Codes are a registry. Ranges:

| Range | Layer |
| --- | --- |
| `EDX-E0xxx` | parse |
| `EDX-E1xxx` | schema and versions |
| `EDX-E2xxx` | references between records |
| `EDX-E3xxx` | geometry |
| `EDX-E4xxx` | relations, rules and constraints |
| `EDX-W....` | warnings, same ranges |

Initial codes:

| Code | Problem |
| --- | --- |
| `EDX-E1001` | unsupported major version |
| `EDX-E1002` | a bundle file's major version differs from the manifest's |
| `EDX-E1003` | a required extension is not supported |
| `EDX-W1004` | a file is of a newer minor version; unknown fields ignored |
| `EDX-E2001` | a part names a package that does not exist |
| `EDX-E2002` | a pad reference names a part that does not exist |
| `EDX-E2003` | a pad reference names a pad its package does not have |
| `EDX-E2004` | a pad belongs to more than one net |
| `EDX-E2005` | a net names a class that `rules.toml` does not define |
| `EDX-E2006` | a duplicate name or id |
| `EDX-E3001` | a ring has fewer than three distinct points, or repeats its first point |
| `EDX-E3002` | a ring intersects itself |
| `EDX-E3003` | a hole is not inside its outer ring, or holes overlap |
| `EDX-E3004` | the stack-up does not start with `top` and end with `bottom` |
| `EDX-E3005` | two pours of different nets overlap at the same priority |
| `EDX-E4001` | an unknown relation kind |
| `EDX-E4002` | a relation kind with no rule |
| `EDX-E4003` | a stated relation id differs from the derived id |
| `EDX-E4004` | an unknown constraint type |
| `EDX-E4005` | two authored constraints contradict each other |
| `EDX-E4006` | an unknown rule key |
| `EDX-E4007` | `max_distance` without `measure` |

## 15. Open issues

These are to be settled before version 1.0 is released.

1. **Rule values for crystals.** `clocks` and `loads` have keys but no recommended values. Values are a design-side
   matter; this specification may give guidance with sources.
2. **Arcs in polygons.** Version 1.0 approximates curved outlines with segments. Native arcs may come in a minor
   version.
3. **Flipping blocks.** Excluded from 1.0, since flipping moves copper between layers.
4. **Further relation kinds.** `terminates` (a series resistor pad to its driver pin) and `pulls` (a pull resistor
   pad to its pin) are expected in a minor version.
5. **Reviewed-board return.** How a layout edited by hand in an EDA tool comes back as a result is not yet specified.
