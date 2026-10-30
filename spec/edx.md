# EDX: Electronics Design eXchange, version 1.0

**Status:** working draft, not yet stable. Breaking changes are still possible before 1.0 is released.

**Licence:** Creative Commons Attribution 4.0 International.

**Schema:** `https://digimap2000.github.io/edx-format/schema/1.0/edx.schema.json`

## 1. Introduction

EDX is a tool-neutral exchange format for printed circuit board design data. One EDX document carries what passes
between a *design side* (a schematic tool, a generator script, an AI agent) and a *layout engine* (a placer and
router, human or automatic):

- **what the board is**: outline, stack-up, packages, parts, nets, keep-out regions and pours;
- **what it must achieve**: relations between pads, rules for each kind of relation and each net class, and
  explicit, checkable constraints;
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
4. **Order-independent.** The meaning of a document never depends on the order in which records appear, and the
   result of merging documents never depends on the order in which they are merged.
5. **Measured the same way everywhere.** Every constraint states how it is measured (section 9), so the design side,
   the engine and any checker agree.
6. **Versioned.** Every document states its EDX version. Within a major version, changes are backwards compatible
   (section 12).

### 1.2 Conformance

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, RECOMMENDED, MAY and OPTIONAL in this
document are to be interpreted as described in BCP 14 (RFC 2119 and RFC 8174) when, and only when, they appear in
all capitals.

This specification defines two roles:

- An **EDX reader** reads documents. A conforming reader MUST apply every check in section 14 that this
  specification marks as an error, and MUST refuse input that fails one.
- An **EDX writer** writes documents. A conforming writer MUST produce documents that pass every check in section
  14, and SHOULD write canonical form (section 13).

The JSON Schema published with this specification is normative for structure. Where the schema and this text
disagree, this text governs and the schema is a defect to be corrected.

### 1.3 Terminology

- **Document:** one EDX file (section 3).
- **Section:** one of the top-level parts of a document: `design`, `rules`, `constraints`, `layout`, `report`.
- **Package:** reusable footprint geometry, independent of any EDA library.
- **Part:** one instance of a package on the board, identified by its reference (`U1`, `C5`).
- **Pad reference:** a part reference and a pad number together, naming one pad on the board.
- **Record:** an item in a document that has an identity (section 3.1).
- **Registry:** a list of names this specification defines and that may grow in a minor version (section 12.2).

## 2. Conventions

### 2.1 Units and precision

- Lengths are in millimetres. Angles are in degrees.
- A reader MUST round every length to the nearest nanometre (0.000001 mm), with ties rounded away from zero, and
  MUST compare lengths only after rounding.
- A reader MUST normalise every angle to the range [0, 360). Angles are compared to 0.000001 degrees.
- Numbers are written in plain decimal notation. A writer MUST NOT write exponent notation, `NaN` or infinities.

### 2.2 Coordinates

- There is one coordinate system for the whole document: x increases to the right and y increases downwards, as
  seen from the top. These are the axes of SVG.
- The origin is wherever the writer puts it. The board outline MAY lie anywhere, including at negative coordinates.
- Positive angles turn counter-clockwise as seen from the top.

### 2.3 Sides and mirroring

- Every package is defined as seen from the top.
- A part on the `bottom` side is its package mirrored left to right (x becomes -x, as seen from the top), then turned
  by its angle, counter-clockwise as seen from the top.
- Mirroring a part to the bottom maps its `top` copper to `bottom` and reverses the order of inner layers.

### 2.4 Points

A **point** is an array of two numbers, `[x, y]`.

### 2.5 Shapes (SVG path data)

Areas (outlines, courtyards, regions, pours, polygon pads) are written as **shapes**: strings in a strict subset of
SVG path data (SVG 1.1, section 8.3). Every EDX shape is valid SVG path data.

- **Commands:** only the absolute commands `M`, `L`, `H`, `V`, `A` and `Z`. Every segment MUST have its own command
  letter; implicit repetition is not allowed.
- **Numbers:** plain decimals as in section 2.1, separated by white space or a comma.
- **Arcs:** circular only. In `A rx ry rotation large-arc sweep x y`, `rx` MUST equal `ry` and both MUST be greater
  than zero. The flags MUST be written as `0` or `1`, separated from their neighbours.
- **Subpaths:** a shape is one or more subpaths. Each starts with `M` and ends with `Z`, and MUST enclose a non-zero
  area without crossing itself.
- **Holes:** the first subpath is the outer boundary. Every later subpath is a hole, which MUST lie inside the outer
  boundary; holes MUST NOT overlap each other.
- **Direction** carries no meaning. A reader MUST accept either. In canonical form an outer boundary runs
  counter-clockwise and a hole clockwise, as seen from the top.
- A field that takes **shapes** takes an array of one or more shapes, which together cover their union.

For geometric checks a reader MAY replace arcs by straight segments, with a chord error of at most 0.0001 mm.
Measured lengths along copper (section 9.2) use exact arc lengths.

Example, a 30 by 20 mm board with rounded corners of radius 1 mm:

```text
M 1 0 A 1 1 0 0 0 0 1 V 19 A 1 1 0 0 0 1 20 H 29 A 1 1 0 0 0 30 19 V 1 A 1 1 0 0 0 29 0 Z
```

### 2.6 Names and references

- A **name** (a part reference, pad number, net, class, layer, package id, region, pour or block name) is a non-empty
  Unicode string in NFC form with no leading or trailing white space and no control characters. Names are
  case-sensitive.
- A pad reference is always written as an object, `{ "part": "C5", "pad": "1" }`. Pad numbers are names, not
  numbers: `"1"`, `"A12"` and `"EP"` are all valid.
- The **display form** of a pad reference is `part.pad` (`C5.1`). It is for people and messages. Because part
  references and pad numbers may themselves contain `.`, a reader MUST NOT parse the display form.
- An **id** built from names (sections 6.1 and 9.5) escapes each name before joining it: `%` becomes `%25`, `.`
  becomes `%2E`, `:` becomes `%3A`, `-` becomes `%2D` and `+` becomes `%2B`.

## 3. The document

An EDX document is one JSON file (RFC 8259), UTF-8 without a byte-order mark. Its RECOMMENDED file name ends in
`.edx.json`.

```json
{
  "edx": "1.0",
  "extensions_used": [],
  "extensions_required": [],
  "design": {},
  "rules": {},
  "constraints": [],
  "layout": {},
  "report": {}
}
```

- `edx` is REQUIRED: the EDX version of the document (section 12.1).
- Every section is OPTIONAL. A document holding only `rules` is valid, as is a design with no constraints.
- `extensions_used` and `extensions_required` are OPTIONAL (section 12.3).
- Every object MAY carry `extras` (section 12.4) and `extensions` (section 12.3).
- A reference is checked only against a list the document has. A part's package is checked when `design.packages`
  is present, a pad reference when `design.parts` is, a net name when `design.nets` is, a layer name when
  `design.board` is, a region name or relation id when `design` is, and a net's class when `rules.classes` is. A partial document, such as constraints written for
  a design held elsewhere, is therefore valid on its own; its references are checked once it is merged or made
  complete (section 4).

| Section | Usually written by | Contents | Section |
| --- | --- | --- | --- |
| `design` | design side | board, packages, parts, nets, regions, pours, relations | 5, 6 |
| `rules` | design side, often by hand | board minimums, net classes, values per relation kind | 8 |
| `constraints` | design side or engine | explicit constraints | 9 |
| `layout` | layout engine | placement, copper, engine regions and pours, groups | 11.1 |
| `report` | layout engine | status of every constraint, and the gates | 11.2 |

### 3.1 Records and identity

These are records, each identified as shown. Identities MUST be unique within their kind.

| Record | Where | Identity |
| --- | --- | --- |
| layer | `design.board.stackup` | `name` |
| package | `design.packages` | `id` |
| part | `design.parts` | `ref` |
| net | `design.nets` | `name` |
| region | `design.regions` | `name` |
| pour | `design.pours` | `name` |
| relation | `design.relations` | relation id (section 6.1) |
| net class | `rules.classes` | its key |
| relation rule | `rules.relations` | its key (the relation kind) |
| constraint | `constraints` | `id` |

### 3.2 Merging documents

Documents MAY be split and merged: a design from one tool and rules from another, say. Merging two documents with the
same major version gives one document:

- `edx` is the higher of the two minor versions.
- `extensions_used` and `extensions_required` are the unions of both lists.
- Each section is merged record by record, by identity. A record present in only one document is kept. A record
  present in both MUST be identical in both (after rounding, section 2.1); otherwise the merge fails with a conflict.
- Values that are not records MUST be identical when both documents have them; otherwise the merge fails with a
  conflict. These are `design.board` as a whole (the stack-up's order matters, so its layers are compared together
  rather than merged one by one), `rules.board`, and any other member not listed in section 3.1, such as a section's
  `extensions` entries and `extras` members, compared per key.
- `layout` and `report` are not merged: a merge of two documents that both have one fails with a conflict.

In the result, the records of each list are ordered by identity, and each conflict is reported as `EDX-E2007` at the
conflicting value in the second document. Merging is therefore commutative and associative: the result never depends
on the order of the documents. The merged document is then checked like any other. A tool
that wants one document's values to replace another's MUST remove the old records first, as a separate step.

## 4. Complete documents

A document may hold any subset of sections, but a layout engine or a checker needs a whole design. A document is
**complete** when:

- `design` has `board`, `packages`, `parts` and `nets`;
- `rules.classes` defines class `default`, with `clearance`, `track_width` and `via`;
- every relation kind used in `design.relations` has a rule in `rules.relations`.

A layout engine or checker MUST refuse an incomplete document. A document with a `layout` or `report` section MUST be
complete.

## 5. Design (`design`)

```json
{
  "board": {
    "outline": ["M 0 0 V 20 H 30 V 0 Z"],
    "stackup": [
      { "name": "top", "role": "signal", "routable": true },
      { "name": "bottom", "role": "signal", "routable": true }
    ]
  },
  "packages": [],
  "parts": [],
  "nets": [],
  "regions": [],
  "pours": [],
  "relations": []
}
```

Every member is OPTIONAL, so that a design can be built from merged documents; section 4 says what a complete one
holds.

### 5.1 Board

- `outline` (shapes, REQUIRED): the board's shape. Holes are cut-outs.
- `stackup` (REQUIRED): the copper layers in order, top first. Each is
  `{ "name", "role": "signal" | "plane", "routable": bool, "plane_net"? }`.
    - There MUST be at least two layers. The first MUST be named `top` and the last `bottom`.
    - A `plane` layer MUST name its `plane_net`. That net fills the layer except where pours of other nets on it
      take precedence (section 5.6), as an island of `+1V8` inside a `+3V3` plane.
    - `routable: false` forbids tracks on the layer.

Any part may go on either side unless a `side` constraint (section 9.2) says otherwise.

### 5.2 Packages

A package is footprint geometry, referenced by its `id` and independent of any EDA library.

- `id` (name, REQUIRED).
- `class` (REQUIRED): from the registry `ic | passive | connector | mechanical | other`. Rules use it, for example to
  decide what counts as a fine-pitch IC.
- `pads` (REQUIRED): each pad has
    - `number` (name, REQUIRED). Two pads in one package MAY share a number; they are then one electrical pad.
    - `kind`: `smd | th | npth | edge`.
    - `shape`: `rect | roundrect | circle | oval | polygon`.
        - `rect`, `circle` and `oval` take `size` `[width, height]` (a circle has equal width and height).
        - `roundrect` takes `size` and `corner_radius`.
        - `polygon` takes `outline`: one shape, relative to the pad's position and before its rotation.
    - `position` (point) and `rotation` (angle, default 0), relative to the package origin.
    - `layers`: `top | bottom | through`.
    - `drill` (REQUIRED for `th` and `npth`): `{ "diameter" }` for a round hole or `{ "slot": [width, height] }`.
    - `zone_connection` (OPTIONAL, overrides the pad's net class): `{ "mode": "solid" | "thermal" | "none",
      "spoke_width"?, "gap"? }`.
- `courtyard` (REQUIRED): `{ "top": shapes, "bottom"?: shapes }`, on the package's own side, with the other side
  OPTIONAL.
- `body` (OPTIONAL): `{ "outline": shapes, "height" }`, for 3D and keep-away checks.

### 5.3 Parts

Each part is `{ "ref", "package", "value"?, "side", "placement", "attributes"? }`.

- `ref` (name, REQUIRED).
- `package` (REQUIRED) MUST be the `id` of a package in the document.
- `side`: `top | bottom`.
- `placement` is one of:
    - `{ "kind": "free" }`: the part has no position until a layout engine gives it one. An engine MUST report a
      free part it could not place as unplaced and MUST NOT give it an arbitrary position.
    - `{ "kind": "fixed", "at": [x, y], "angle" }`: the position is mandated (connectors, fixings, marks). The
      part's position is that of its package origin.
- `attributes` (OPTIONAL): a map of names to strings, carried but not interpreted by this specification.

### 5.4 Nets

Each net is `{ "name", "pads": [pad reference, ...], "class"? }`.

- Every pad reference MUST name an existing part and a pad number in its package.
- A pad belongs to at most one net. A pad in no net is unconnected.
- `class` names a net class defined in `rules` (section 8.2). When absent, the net is in class `default`.

### 5.5 Regions

A region is a named area that excludes things.

- `name` (REQUIRED), `outline` (shapes, REQUIRED).
- `layers`: an array of layer names, or `"all"`.
- `excludes`: one or more of `parts | pads | tracks | vias | pours`.
- `except_nets` (OPTIONAL): nets whose copper the region allows.
- `owner` (OPTIONAL): `{ "part": ref }` or `{ "block": name }`. An owned region is given in its owner's coordinates
  and moves with it. Without an owner it belongs to the board.

### 5.6 Pours

A pour is copper fill as design intent:
`{ "name", "net", "layers", "outline", "priority", "connection"?, "clearance"? }`.

- On a signal layer, a pour is, for example, a ground fill.
- On a plane layer, a pour of another net is an island in the plane.
- A plane layer's own net (section 5.1) has priority 0 and fills what the layer's pours leave. Every pour's
  `priority` MUST be at least 1.
- Where two pours overlap, the higher `priority` wins. Two overlapping pours of different nets with the same priority
  are an error.
- `connection` (as `zone_connection` in section 5.2) and `clearance` default to those of the pour's net class.

## 6. Relations

A relation records design intent between two pads, with no values:

```json
{ "kind": "decouples", "subject": { "part": "C5", "pad": "1" }, "object": { "part": "U1", "pad": "9" } }
```

### 6.1 Identity

- A relation's id is `kind:subject:object`, each pad reference in display form and each name escaped as in section
  2.6: `decouples:C5.1:U1.9`.
- A writer MAY state the id as `id`; a reader MUST derive it and MUST refuse a stated id that differs.
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

A relation of a kind that is neither registered nor provided by a supported extension is an error.

## 7. Blocks

A block is a set of parts, with their copper and regions, frozen relative to each other and handled as one rigid
body: it moves and turns as one, and does not flip in version 1.0.

- The design side requests a block with a `block` constraint (section 9.2). A layout engine fulfils the request and
  reports the block in its layout (section 11.1). An engine MAY also form blocks of its own.
- A block's **courtyard** is the union of its members' courtyards and its own regions.
- A block's **pins** are the pads whose nets leave the block.
- Constraints between a block's members still apply and are re-measured in every report.
- Allowed turns are given by an `orientation` constraint on the block.
- Blocks MAY nest.

## 8. Rules (`rules`)

```json
{
  "board": { "copper_to_edge": 0.3, "hole_to_hole": 0.25 },
  "classes": {
    "default": {
      "clearance": 0.15,
      "track_width": { "min": 0.15, "preferred": 0.2 },
      "via": { "pad": 0.6, "drill": 0.3 }
    },
    "Supply": {
      "track_width": { "min": 0.3, "preferred": 0.5 },
      "layers": { "bottom": { "track_width": { "min": 0.4, "preferred": 0.6 } } }
    }
  },
  "relations": {
    "decouples": { "max_track": 2.5, "square_on_pin": true, "via": "beside_pad", "adjacent_pins_via": "beside_pad" },
    "backs": { "max_track": 6 }
  }
}
```

### 8.1 Board minimums

`board` holds board-wide minimums: `copper_to_edge` and `hole_to_hole`.

### 8.2 Net classes

- `classes` maps each class name to its electrical rules: `clearance`, `track_width` (`min` and `preferred`), `via`
  (`pad` and `drill`), and per-layer overrides under `layers.<layer name>`.
- Every class a net names MUST be defined. A complete document defines class `default` in full (section 4).
- Any other class takes each value it omits from `default`.

### 8.3 Relation rules

`relations` gives the values and style for each relation kind. The keys each kind accepts are part of that kind's
registry entry:

| Kind | Keys |
| --- | --- |
| `decouples` | `max_track`, `square_on_pin` (bool), `via`, `adjacent_pins_via` (each a `via_policy` value, section 9.2) |
| `backs` | `max_track` |
| `filters` | `max_track` |
| `feeds` | `max_track` |
| `clocks` | `same_layer` (bool), `no_vias` (bool), `matched_tolerance`, `quiet_margin` |
| `loads` | `max_track` |
| `near` | `max_distance`, `measure` (`centre` or `edge`) |

An unknown key is an error. Every key is OPTIONAL: an absent key derives no constraint.

## 9. Constraints (`constraints`)

```json
[
  {
    "id": "max_track:C5.1:U1.9",
    "type": "max_track",
    "operands": [{ "part": "C5", "pad": "1" }, { "part": "U1", "pad": "9" }],
    "value": 2.5,
    "origin": "derived",
    "source": { "relation": "decouples:C5.1:U1.9", "rule": "decouples.max_track" }
  }
]
```

### 9.1 Common fields

- `id` (REQUIRED): stable across versions of the design. Reports refer to constraints by id.
- `type` (REQUIRED): from the registry in section 9.2.
- `operands`, `value`, `measure`: as the type defines.
- `hard`: whether the constraint must hold. It defaults to `true`, except for `square_on_pin` (see below).
- `cost`: for a soft constraint, the cost per unit of violation in millimetre-equivalents, on the same scale as
  wirelength. Default 1. Anything that must hold is hard; soft constraints should be few.
- `origin` (REQUIRED): `derived` (from a relation and a rule) or `authored`.
- `source` (REQUIRED): `{ "relation", "rule" }` for a derived constraint, or `{ "document" }` (free text or a URI)
  for an authored one.

### 9.2 Constraint types

The type list is a registry. Registered types:

| Type | Operands | Value | Measured |
| --- | --- | --- | --- |
| `max_track` | pad, pad | length | pad centre to pad centre. Once joined: along the copper from where each track meets each pad, on to the pad centres, vias counting zero. Before: the straight line between centres. |
| `max_distance` | pad or part, pad or part | length | straight line. `measure: "centre" \| "edge"`, REQUIRED. |
| `max_length` | net or path | length | routed length |
| `matched` | two or more nets | tolerance | routed lengths equal within the tolerance |
| `square_on_pin` | part, pad | tolerance, OPTIONAL | the part's pad on the pin's line, facing it. Soft by default, costed by sideways offset. Hard when a value is given, the offset held within it. |
| `via_policy` | relations | `in_link \| beside_pad \| none` | `in_link`: room for a via between the pin's tip and the pad, the via there. `beside_pad`: beside the pad, pointing away from the link. `none`: the link has no via. |
| `same_layer` | nets or paths | none | all copper on one layer |
| `no_vias` | nets or paths | none | no vias |
| `quiet` | parts and nets | margin | other nets' copper kept the margin from the group, on every layer |
| `keepout` | region | none | as section 5.5 |
| `fixed` | part | `{ "at", "angle" }` | position and angle as given |
| `side` | part | `top \| bottom` | the part on that side |
| `orientation` | part or block | angles | one of the allowed angles |
| `block` | parts | `{ "name" }` | the parts and their copper form a rigid block (section 7) |

Operands are objects that say what they name:

| Operand | Form |
| --- | --- |
| pad | `{ "part", "pad" }` |
| part | `{ "part" }` |
| net | `{ "net" }` |
| block | `{ "block" }` |
| region | `{ "region" }` |
| path | `{ "path": [pad reference, ...] }`, an ordered run of two or more pads |
| relations | `{ "relations": [relation id, ...] }` |

### 9.3 Measures

Every constraint type states how it is measured. A type whose measure has options (`max_distance`) MUST state
`measure`. Checkers MUST measure as stated.

### 9.4 Status

A constraint is `met`, `broken`, or `not_yet_measurable` (for example, a `matched` group not yet routed).

### 9.5 Derivation and precedence

- Constraints MAY be derived from relations and rules, by the design side or by a layout engine.
- A derived constraint's id is its `type`, then each operand in order, joined by `:`. A pad is written in display
  form, a path as its pads joined by `-`, a relation group as its ids joined by `+`, and any other operand as its
  name. Each name is escaped as in section 2.6.
- An authored constraint wins over a derived constraint with the same id.
- Two authored constraints that contradict each other are an error. They contradict when no layout could meet both:
  for example, two `fixed` constraints on one part with different positions, or `side` constraints naming different
  sides.

## 10. Placement geometry

- A part's position is the position of its package origin. Its angle is the turn of the package, after mirroring
  for the bottom side (section 2.3).
- A pad's position on the board follows from the part's position, side and angle, and the pad's position and
  rotation in its package.

## 11. Results

A layout engine returns the document it was given with `layout` and `report` sections added. Each records the hashes
of the sections it was made from (section 13.2), so a result can be matched to its inputs:

```json
"inputs": { "design": "sha256:…", "rules": "sha256:…", "constraints": "sha256:…" }
```

### 11.1 Layout (`layout`)

- `inputs`: as above.
- `placement`: per part `{ "ref", "placed", "at"?, "angle"?, "side"?, "locked"? }`. A placed part has `at`,
  `angle` and `side`; an unplaced part has `placed: false` and none of them.
- `copper`:
    - `tracks`: `{ "net", "layer", "width", "start", "end", "locked"? }`;
    - `arcs`: `{ "net", "layer", "width", "start", "mid", "end", "locked"? }`;
    - `vias`: `{ "net", "at", "pad", "drill", "layers": [from, to], "locked"? }`.
- `regions` and `pours`: those the engine added, in the form of sections 5.5 and 5.6, kept apart from the design's.
- `groups`: `blocks` (`{ "name", "parts", "angles"? }`) and `quiet` (`{ "name", "nets", "margin" }`).

`locked` (default `false`) marks an item the engine was told not to move or remove.

### 11.2 Report (`report`)

- `inputs`: as above.
- `constraints`: per constraint, `{ "id", "status", "measured"?, "limit"?, "measure"?, "hard", "cost"?,
  "where"? }`, where `where` names the parts, nets or pads involved and MAY give a point.
- `unplaced`: the references of parts that are not placed.
- `overlaps`: pairs of parts or blocks whose courtyards overlap.
- `unrouted`: per net, the pads not yet connected.
- `clearance`: clearance violations found by the engine's own check, each with its layer, a point, the items
  involved, and the actual and required distances.
- `gates`: `{ "constraints_met", "all_placed", "all_routed", "clearance_clean" }`, each a boolean.

Every broken hard constraint MUST be listed. Unplaced parts are reported separately from overlaps.

## 12. Versioning and extensions

### 12.1 Versions

- An EDX version is `MAJOR.MINOR`, stated in the document's `edx` member.
- A minor version only adds: optional members, registry entries, error codes. A reader for version `M.n` MUST read a
  document of version `M.m` for any `m`. Where `m > n`, it MUST ignore members it does not know, warn, and still
  apply every check it knows.
- A major version MAY make incompatible changes. A reader MUST refuse a document with a major version it does not
  support.
- Editorial corrections to this text that change no requirement are published as `MAJOR.MINOR.PATCH` of the
  specification only. Documents never state a patch number.

### 12.2 Registries

Package classes (5.2), relation kinds (6.2), relation rule keys (8.3), constraint types (9.2) and error codes (14)
are registries. Registered names are bare words. A minor version MAY add to a registry and MUST NOT remove from or
change an entry.

### 12.3 Extensions

- An extension is named `<namespace>.<name>`, such as `acme.shield_can`. The namespace is a name its author
  controls, such as a company or project name.
- An extension MAY add registry entries (a relation kind or constraint type with the extension's name) and MAY add
  data to any object, inside that object's `extensions` member, keyed by the extension's name:
  `"extensions": { "acme.shield_can": { "height": 2.5 } }`.
- `extensions_used` lists every extension the document uses. `extensions_required` lists those a reader must
  understand to read the document correctly, and MUST be a subset of `extensions_used`.
- A reader MUST refuse a document that requires an extension it does not support. A reader MAY ignore an extension
  that is used but not required, and SHOULD warn when it does.
- An extension MAY be proposed for registration, which makes its names bare in a later minor version.

### 12.4 Extras

Every object MAY carry an `extras` member holding a JSON object. Readers MUST preserve `extras` when they rewrite a
document and MUST NOT give it meaning. Tools use it for data that is theirs alone; data other tools need belongs in an
extension.

## 13. Canonical form and hashing

### 13.1 Canonical form

A document in canonical form:

- has object members in the order of the schema, with `extensions` and then `extras` last;
- omits members whose value equals their default, and empty optional arrays and objects;
- sorts arrays whose order has no meaning by identity (section 3.1), and pad references by part, then pad; constraint
  operands and path pads keep their order;
- writes numbers in the shortest plain decimal that rounds to the same value (section 2.1), with no trailing zeros
  and no trailing decimal point;
- writes shapes with single spaces between tokens, outer boundaries counter-clockwise and holes clockwise;
- indents by two spaces, uses LF line endings, and ends with a single newline.

### 13.2 Hashing

The hash of a section is `sha256:` followed by the lowercase hex SHA-256 digest of that section's value written in
canonical form, as a JSON document on its own. Two sections with the same meaning have the same hash, whatever their
formatting or order.

## 14. Validation

### 14.1 Layers

A reader checks a document in four layers, in order, and stops after the first layer that reports an error:

1. **Parse:** UTF-8 without a byte-order mark, JSON syntax, and no object with two members of the same name.
2. **Version and extensions:** `edx` present and of a supported major version; every required extension supported
   (section 12).
3. **Schema:** structure, types, required members and allowed values, against the published JSON Schema. A
   document of a newer minor version is checked with unknown members allowed (section 12.1).
4. **Semantics:** every rule in this text that a schema cannot express. A reader reports every semantic error it
   finds, not only the first.

Warnings never stop a reader.

### 14.2 Diagnostics

Every problem a reader reports carries a code, a severity (`error` or `warning`), a JSON Pointer (RFC 6901) to the
value at fault, and a message. A reader SHOULD add line and column.

The pointer locates:

- for a reference to something that does not exist, the member holding the name (`/design/parts/3/package`);
- for two records sharing an identity, the later record's identity member, in document order;
- for a missing member, the object that should hold it;
- for a problem with a whole shape, list or object, that value;
- for a merge conflict, the conflicting value in the second document;
- for a parse error, the empty pointer (the whole document).

For a schema error (`EDX-E1000`), validators differ in how deep they point, so the pointer MUST be the object
holding the fault or a value inside it.

### 14.3 Codes

Codes are a registry. Ranges:

| Range | Topic |
| --- | --- |
| `EDX-E0xxx` | parsing |
| `EDX-E1xxx` | schema, versions and extensions |
| `EDX-E2xxx` | references, identities, completeness and merging |
| `EDX-E3xxx` | geometry |
| `EDX-E4xxx` | relations, rules and constraints |
| `EDX-Wxxxx` | warnings, in the same ranges |

Initial codes:

| Code | Layer | Problem |
| --- | --- | --- |
| `EDX-E0001` | parse | not valid JSON |
| `EDX-E0002` | parse | not UTF-8, or begins with a byte-order mark |
| `EDX-E0003` | parse | an object has two members with the same name |
| `EDX-E1000` | schema | the document does not match the schema |
| `EDX-E1001` | version | `edx` is missing or is not a version |
| `EDX-E1002` | version | unsupported major version |
| `EDX-E1003` | version | a required extension is not supported |
| `EDX-E1004` | version | `extensions_required` names an extension missing from `extensions_used` |
| `EDX-E1005` | semantics | an extension name is used in the document but missing from `extensions_used` |
| `EDX-W1006` | version | the document is of a newer minor version; unknown members are ignored |
| `EDX-W1007` | semantics | an extension that is used but not required is not supported; its data is ignored |
| `EDX-E2001` | semantics | a part names a package that does not exist |
| `EDX-E2002` | semantics | a reference names a part that does not exist |
| `EDX-E2003` | semantics | a pad reference names a pad its part's package does not have |
| `EDX-E2004` | semantics | a pad belongs to more than one net |
| `EDX-E2005` | semantics | a net names a class that is not defined |
| `EDX-E2006` | semantics | two records of one kind share an identity |
| `EDX-E2007` | merge | merge conflict: one identity, two different records or values |
| `EDX-E2008` | semantics | the document is not complete (section 4) where it must be |
| `EDX-E2009` | semantics | a stated relation id differs from the derived id |
| `EDX-E2010` | semantics | a reference names a net that does not exist |
| `EDX-E2011` | semantics | a reference names a layer that does not exist |
| `EDX-E2012` | semantics | a reference names a region, block or relation that does not exist |
| `EDX-E3001` | semantics | an arc whose radii differ, or are zero |
| `EDX-E3002` | semantics | a subpath crosses itself or encloses no area |
| `EDX-E3003` | semantics | a hole is not inside its outer boundary, or holes overlap |
| `EDX-E3004` | semantics | the stack-up does not start with `top` and end with `bottom` |
| `EDX-E3005` | semantics | two pours of different nets overlap on a layer at the same priority |
| `EDX-E3006` | semantics | a circular pad whose width and height differ |
| `EDX-E4001` | semantics | a relation kind with no rule, in a document that must be complete |
| `EDX-E4002` | semantics | two authored constraints contradict each other |

Block names are those given by `block` constraints (section 9.2) and by `layout.groups.blocks`.

## 15. Open issues

These are to be settled before version 1.0 is released.

1. **Rule values for crystals.** `clocks` and `loads` have keys but no recommended values. Values are a design-side
   matter; this specification may give guidance with sources.
2. **Flipping blocks.** Excluded from 1.0, since flipping moves copper between layers.
3. **Further relation kinds.** `terminates` (a series resistor pad to its driver pin) and `pulls` (a pull resistor
   pad to its pin) are expected in a minor version.
4. **Reviewed-layout return.** How a layout edited by hand in an EDA tool comes back as a `layout` section is not yet
   specified.
5. **Rule key names** for `clocks`, `loads` and `near` are provisional.
