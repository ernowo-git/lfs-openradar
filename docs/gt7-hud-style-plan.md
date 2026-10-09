# GT HUD style

Status: implemented and revised against the user's five visual references,
renamed to GT and selected by default when no style is saved.

The saved `HudStyle` enum and `theme.rs` resolver select the existing Classic
constants or the separate `gt7_style.rs` bundle. Painting and display formatting
now live in `render.rs`; `desktop.rs` manages controls and native viewports.
The Gadgets tab includes the global selector and actual gadget previews. Both
themes use their own base window dimensions. The original
layout test's stale position-mode label lookup has also been corrected.

## Objective

Add a selectable **GT** style for Radar, Gap Ahead, Gap Behind, and
Performance Delta. Keep the current appearance available as **Classic**, and keep
GT as the default for new installations and configuration files without a saved style.
Preserve the current style constants, including local edits to colors and borders.

The first version should use one global **HUD style** selector. Each gadget keeps
its independent position, scale, enabled state, and position mode. Per-gadget style
overrides can be added later if needed.

## Official references

The subsequent visual revision takes **Frame 6.png** through **Frame 10.png**
provided by the user as the primary appearance references. Following the user's
subsequent correction, the blue backing is transparent in the overlay. Frame 7 supplies the joined
BEST/time/delta strip; Frame 8 supplies boxed positions and fading interval rows.
Frames 6 and 9 guide numeric and label typography, and Frame 10 guides timing
colors and millisecond formatting. The existing gadgets use these visual details;
this change does not introduce a speedometer, lap-history table, or full leaderboard.
The subsequently supplied `ezgif.com-optimize.gif` is the primary proximity-radar
reference, including its cropped/fading arcs, compact markers and bottom
label. The blue backing remains transparent; a subsequent request adds a radial
black-to-grey background with 40% centre opacity fading to 0% at the outer circle.
The earlier request to remove the vertical center strip still applies.

Research inspected the official manual's actual screenshots, rather than using a
third-party recreation as the design reference:

- [GT7: The Race Screen](https://www.gran-turismo.com/us/gt7/manual/race/02)
  — [annotated screenshot](https://www.gran-turismo.com/images/c/i1B1lEMx5qJnOb.png).
- [GT7: The Multi-Function Display](https://www.gran-turismo.com/us/gt7/manual/race/04)
  — [radar screenshot](https://www.gran-turismo.com/images/c/i1xA6ggV8CaXd.jpg)
  and [Session Best screenshot](https://www.gran-turismo.com/images/c/i1TfTSwn7LjFiG.jpg).
- [GT7: Display Settings](https://www.gran-turismo.com/us/gt7/manual/drivingoption/05)
  — context for race information and driver-list display options.

The screenshots illustrate a particular view and race state. They are visual
references, not a specification for every GT7 mode or update. Exact proprietary
font names and official RGB values have not been established.

## Observed visual language

| Element | Observed in the official screenshots |
| --- | --- |
| Typography | White sans-serif labels; distinctive large numeric readouts; smaller units and supporting labels |
| Surfaces | Transparent or translucent dark backing; compact rectangular timing cells and driver rows |
| Lines | Fine white/gray rules, meter ticks, and circular radar guides |
| Driver list | Aligned position/name rows; a pale highlighted player row; separate timing information |
| Radar | Concentric circles and crosshair, a translucent vertical center band, red player marker, blue nearby-car markers |
| Car markers | Small directional arrow-like markers, rather than rectangular car footprints |
| Session Best comparison | Blue cells for negative/faster differences and red cells for positive/slower differences |
| Record emphasis | Green emphasis for a session best in the timing table; purple best-time emphasis elsewhere in the race HUD |

These observations do not imply one universal good/bad color rule across the HUD.
In particular, blue timing improvements and green record highlights have different
roles. GT7's radar screenshot also does not establish OpenRadar's three proximity
threat categories.

## Proposed OpenRadar adaptation

The following is a design proposal, not a claim that GT7 has identical gadgets or
telemetry behavior.

### Radar

- Fine pale arcs fading toward the top and a horizontal guide. Guide placement
  follows the animation, with a radial black-to-grey background fading from
  40% centre opacity to fully transparent at the outer circle. Colors and both
  opacity endpoints are editable in the theme's radial background settings.
- Keep the center clear; the cosmetic vertical band was removed after review.
- A red directional player marker and blue directional opponent markers.
- Preserve OpenRadar's warnings using left/right sectors of the outer circle:
  amber for nearby/alongside cars, red for potential contact. Use the strongest
  live threat per side; keep confident opponent borders pale blue. Sector
  thickness, angular span, opacity, outline and colors are theme settings.
- Muted gray treatment for uncertain opponents.
- A bottom `Radar` label and compact unavailable/paused text.
- Preserve actual relative positions, headings, detection ranges, and car
  dimensions in the telemetry engine. Use compact directional symbols with
  configurable size in the visual theme; scale them with the window. Symbol
  positions use independent linear horizontal and vertical scales, fitted to
  the configured side and front/rear ranges. Side-range zoom is never capped
  by the longitudinal range. Guide arcs are decorative, not metre circles.

This requires a marker shape option and radar guide styling, not only a palette
change. Classic uses uniform zoom controlled by side range, preserving rectangle
proportions, circular metre guides, and uniform inner borders.

### Gap Ahead and Gap Behind

- Adapt the driver-list language into compact horizontal rows.
- Small `AHEAD` / `BEHIND` label, position/name information, and a prominent
  right-aligned gap value.
- Transparent backing, separate bordered position box, grey-black name cell, fading grey-black
  interval row, square corners, and pale primary text.
- Hide passage-history status and estimate age by default. Show them when the
  saved `hud_debug` setting is enabled through **HUD debug information**. The
  same debug toggle applies to Classic gap gadgets through the shared renderer.
  Unavailable gap values still display a placeholder.
- Keep the existing meaning of ahead/behind gaps and lap differences; do not infer
  lap improvement from a race gap.

GT7's driver list inspires this layout. The two independent gap windows are an
OpenRadar adaptation, not a direct reproduction of a GT7 gadget.

### Performance Delta

- Grey-black reference-time cell with outlined Orbitron digits and a separate
  signed delta cell, adapting Frame 7 without the green `BEST` square.
- Blue text for faster/negative delta, red text for slower/positive delta, and
  white/gray for neutral or unavailable values. Reference and live-delta cells,
  plus gap position/name cells, use neutral grey-black at 30% opacity.
- A larger 32-pixel estimated-lap readout beneath the timing cells; no colored
  trend meter or gaining/losing/estimate-status footer in this theme.
- The reference remains the actual session-best lap, and the signed delta
  remains the live comparison. Unavailable times show a placeholder.
- Preserve comparison thresholds, calculation, reference behavior, and status
  handling from the existing implementation.

GT7 base sizes are gaps 320 x 106 and delta 440 x 132 logical pixels. Classic
retains gaps 230 x 76 and delta 230 x 100. The shared cell and text descriptors
keep geometry, fills, borders, fonts and outlines editable in `gt7_style.rs`.
The palette approximates the supplied images, rather than claiming official RGB
values. Millisecond display formatting preserves the existing engine calculations.

## Implementation design

1. Add a UI-independent `HudStyle` enum to configuration, serialized as `classic`
   and `gt`, accepting `gt7-inspired` as a legacy alias. Its default is GT;
   explicit Classic selections are retained. Keep this enum usable without the desktop feature.
2. Add a desktop theme resolver returning an immutable bundle of radar, ahead,
   behind, and delta styles for the selected enum value.
3. Compose the Classic bundle from the existing `radar_style.rs`, `gap_style.rs`,
   and `delta_style.rs` constants. Add a separate GT7 theme definition; do not
   overwrite the existing constants or duplicate the telemetry engines.
4. Extend the existing style types only for capabilities the design needs:
   radar marker shape, radar guides/labels, text alignment, timing-cell
   treatment, and row separators. Introduce a small theme-specific painting
   function where the layout genuinely differs; avoid a large collection of
   boolean flags or a trait hierarchy for two built-in themes.
5. Pass resolved styles into painting functions. Use the same resolved base size
   for both native viewport builders and canvas fitting. Remove direct Classic
   constant lookups from paths that need to honor the selection.
6. Add a global `HUD style` dropdown with `Classic` and `GT` choices,
   placed near the gadget controls. Apply changes immediately and persist them
   through the existing **Save settings** action.
7. Ensure retained child viewport callbacks read the current selection at paint
   time. Update every enabled child after a switch, including when the control
   panel is not repainting. Reuse existing resize debounce behavior if a theme
   changes window dimensions.
8. Keep positions, user scales, visibility, hotkeys, focus behavior, click-through
   behavior, and position mode consistent when switching themes.

Use the existing egui renderer with bundled OFL-licensed Roboto and Orbitron
families for this theme. Classic's font families stay unchanged. Sources and
hashes are in `assets/fonts/README.md`; full notices are in `docs/font-licenses.md`
and travel with release archives through the existing documentation packaging.
Shapes are drawn with code; the supplied screenshots are not shipped as assets.

## Delivery sequence

1. Review a visual comparison of Classic and the proposed GT gadgets
   with identical sample telemetry, including warnings and unavailable states.
2. Implement theme selection, persistence, and the two theme bundles.
3. Add radar geometry and gap/delta layout treatments for the additional theme.
4. Verify both themes and document the selector and customization locations.

## Acceptance checks

- Existing configuration files load as Classic; both selections round-trip
  through TOML.
- Classic preserves the current appearance and user-edited constants.
- Switching in both directions refreshes all gadgets without reconnecting LFS,
  resetting placement, or changing enabled state.
- Native window dimensions and painter fitting use the same selected style.
- Check both themes at minimum/maximum scale and radar size, with long names,
  lap gaps, stale telemetry, missing reference laps, and all proximity threats.
- Signed delta values, estimate labels, and uncertainty information remain clear.
- Compare actual screenshots on bright and dark game backgrounds; inspect text,
  clipping, opacity, and warning visibility.
- Verify the desktop build and the build without default features.
- Run relevant config, overlay, geometry, and retained-viewport tests. The
  previously failing `opening_panel_fits_cards_and_tabs_keep_actions_in_the_header`
  test now looks up the actual **Radar position mode** label, and passes.

No version bump, release, or pull request is part of this research and plan.
