# Theme contract — SpliceCraft.rs UI chrome

Shared visual contract for post-parity stages **17–20**. Ratatui will never be
pixel-identical to Textual; this document pins **colors, density, and chrome
roles** so the workbench reads as the same product.

Upstream sources (fetch with `gh`, do not vendor):

| Upstream | What to copy |
|---|---|
| `splicecraft_util.py` → `_DEFAULT_TYPE_COLORS` | Feature-type → hex |
| `splicecraft_widgets.py` → `_FEATURE_PALETTE`, `_resolve_feature_color` | Fallback palette + resolve order |
| `splicecraft.py` → `PlasmidApp.CSS`, `LibraryPanel` / `FeatureSidebar` / `SequencePanel` / `MenuBar` DEFAULT_CSS | Layout sizes, focus brighten, menu/footer |
| `splicecraftScreenshot.png` | Density / color reference |
| `docs/keybindings.md` | Footer shortcut strip + menu list |

Rust home for the port: `crates/splicecraft-tui` (`theme` module + styled
draw path). Paint only — do not change scan / `rc` / wrap / persist.

## Intentional differences (keep)

- Toolkit is **Ratatui**, not Textual. No TCSS, no Textual widgets.
- Data dir remains `splicecraft-rs` (never the Python leaf).
- Welcome / about chrome says **SpliceCraft.rs**.

## Palette

### Chrome

| Role | Target | Notes |
|---|---|---|
| Screen / panel background | near-black | Upstream focus brighten uses `#0c0c0c` (~5% above pure black). |
| Focused panel background | `#0c0c0c` | Subtle; do not wash out braille. |
| Unfocused border | dark gray | Thin box borders. |
| Focused border | primary / accent | Cyan-family is fine if it matches status/menu. |
| Menu bar | primary-darken background; active item primary + contrast text | One row; labels match `MenuBar.MENUS`. |
| Status / footer | high-contrast strip | Shortcut cheat sheet, not `stage NN`. |
| Selected library row | primary bg + dark fg | Match upstream cyan selection. |
| Toast / warning overlay | yellow border | Collision / confirm. |
| Destructive overlay | red border | Master Delete. |

### Feature-type defaults

Port exactly from upstream `_DEFAULT_TYPE_COLORS` (hex). Resolve order matches
`_resolve_feature_color`:

1. Feature / library entry `color` qualifier (if valid hex / named / `color(N)`).
2. User overrides from settings / feature-colors store (when present).
3. Built-in type map below.
4. `_FEATURE_PALETTE[0]` (`color(39)` → hex via xterm-256).

```
CDS             #FFA500
gene            #FFD700
mRNA            #FFA07A
tRNA            #FF69B4
rRNA            #FF1493
ncRNA           #DA70D6
misc_RNA        #BA55D3
promoter        #00CED1
terminator      #DC143C
RBS             #00FF7F
polyA_signal    #FF6347
regulatory      #7FFFD4
5'UTR           #87CEEB
3'UTR           #4682B4
intron          #A9A9A9
exon            #90EE90
operon          #DDA0DD
primer_bind     #00BFFF
protein_bind    #F08080
misc_binding    #FF8C00
repeat_region   #CD853F
LTR             #8B4513
mobile_element  #8B008B
rep_origin      #9370DB
oriT            #BA55D3
sig_peptide     #ADFF2F
mat_peptide     #9ACD32
transit_peptide #7CFC00
propeptide      #6B8E23
misc_feature    #20B2AA
misc_recomb     #48D1CC
stem_loop       #FF4500
variation       #800080
```

Fallback palette indices (xterm-256 `color(N)`), same order as upstream
`_FEATURE_PALETTE`:

`39, 118, 208, 213, 51, 220, 196, 46, 201, 129, 166, 33, 226, 160, 87, 105, 154, 203, 81, 185`

### Sequence panel accents

| Element | Color role |
|---|---|
| AA translation letters | Bright green (CDS) |
| Bases under a feature | That feature’s resolved color |
| Restriction name / span | Magenta / enzyme accent |
| Cursor caret | High-contrast marker |
| Sticky-cut up/downstream tint | Blue / red — stage 20 if not earlier |

## Layout density

Upstream CSS targets (approximate; Ratatui uses constraints):

| Pane | Upstream | Rust target |
|---|---|---|
| Library | `width: 32` | ~32 columns (not a soft 22%) |
| Features | `width: 32` | ~32 columns |
| Map | `1fr` in top row | Remaining width after side panes |
| Sequence | `height: 14` | Fixed ~14 rows (full width under top row) |
| Menu | `height: 1` | 1 row |
| Footer | `height: 1` | 1 row shortcut strip |

Top row = Library | Map | Features. Sequence under it spans the window.

## Chrome roles

1. **Menu bar** — 16 labels (`File` … `BABS`). Activatable (highlight + open tool). File may stay palette/path-driven until a dropdown exists.
2. **Library** — collection name, scrollable list, selected row, search line, action affordances (+ / − / edit as space allows).
3. **Map** — braille / ASCII ring or linear backbone; **colored** feature labels and RE ticks.
4. **Features** — table-like Type | Label (and bp/strand when space); type-colored text.
5. **Sequence** — ruler, feature lane, top strand, AA lane, bottom strand, caret — styled spans.
6. **Footer** — bindings like upstream (`^q Quit`, `f Fetch`, `^o Open`, `? Help`, …), toast overrides temporarily.

## Render architecture

Stage 05 shipped pure `Record → Vec<String>`. Theme stages must add a styled
path without breaking geometry tests:

- Keep geometry pure and unit-tested.
- Prefer `render_map_styled` / `render_sequence_styled` → `Vec<Line<'static>>`
  (or owned spans) that `draw.rs` paints.
- Plain `Vec<String>` may remain for ASCII fallbacks / tests that only check
  glyphs and midpoints.

## Stages that consume this doc

| Stage | Scope |
|---|---|
| [17](stages/17-theme-chrome.md) | Theme module, type colors, styled map/seq/sidebar/menu/footer |
| [18](stages/18-layout-density.md) | Fixed pane sizes, focus brighten, overlay polish |
| [19](stages/19-interaction-keys.md) | Missing keys, menu activation, fetch / new / find / clipboard |
| [20](stages/20-mouse-polish.md) | Mouse, sticky-cut tint, remaining chrome |

Update [`parity.md`](parity.md) when a visual or keybinding **gap** closes.
