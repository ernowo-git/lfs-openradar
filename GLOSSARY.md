# OpenRadar

OpenRadar displays driving information from Live for Speed through independently configurable HUD gadgets.

## Language

### HUD configuration

**HUD**:
OpenRadar's collection of driving-information gadgets displayed over the game.

**Gadget**:
One independently positioned and scaled HUD element: Radar, Gap Ahead, Gap Behind, Performance Delta, Speed dashboard, or Fuel.

**HUD style**:
The selected visual appearance of the gadgets, currently Classic or GT.
_Avoid_: HUD layout when referring only to appearance.

**GT style**:
The existing GT-inspired appearance of Radar, Gap Ahead, Gap Behind, and Performance Delta. Speed dashboard and Fuel retain their own appearance in either HUD style.
_Avoid_: GT layout when referring only to appearance.

**HUD layout**:
The chosen positions and individual gadget scales of the gadgets, including disabled gadgets. HUD style and gadget enablement are separate preferences.

**Saved HUD layout**:
The most recently explicitly saved arrangement of gadget positions and chosen gadget scales.

**Position mode**:
The editing mode used to adjust a gadget's placement.

**Overlay backend**:
The presentation mechanism drawing the HUD: independent Windows/X11 gadget
windows or a grouped native Wayland overlay surface.

**Backend preference**:
The saved selection of Auto, Window, or Native Wayland. Auto resolves to an
active overlay backend at startup and remains Auto when settings are saved.
_Avoid_: Active backend when referring to the saved preference.

### Placement and sizing

**Selected monitor**:
The monitor chosen to display the HUD, whether currently connected or unavailable.

**Monitor-relative position**:
A gadget's top-left position in logical pixels measured from the selected monitor's top-left corner.
_Avoid_: Desktop position when referring to monitor-relative placement.

**Requested position**:
A gadget's chosen position before temporary constraints from the selected monitor's bounds. It can reflect saved placement or an unsaved edit.
_Avoid_: Displayed position when referring to the user's chosen coordinates.

**Displayed position**:
The position used to present a gadget after fitting its placement within the selected monitor's bounds.
_Avoid_: Saved position when referring to temporary placement.

**Gadget scale**:
The requested size multiplier chosen for an individual gadget, independent of display scale and temporary fitting.
_Avoid_: Displayed gadget scale or display scale when referring to the chosen size.

**Displayed gadget scale**:
The scale at which a gadget is currently visible. It may be smaller than its chosen gadget scale when the selected monitor cannot fit the requested size.

**Display scale**:
The monitor's scaling factor, such as 125% or 150%, that maps logical HUD dimensions to screen pixels.

### Visibility

**Gadget enablement**:
The individual preference determining whether a gadget belongs in the active HUD, independent of its placement and current visibility.
_Avoid_: Gadget visibility when referring only to this preference.

**Show overlays**:
The user's manual preference to show or hide the HUD as a whole, independent of individual gadget enablement.

**Visible gadget**:
A gadget currently presented on screen. An enabled gadget may be hidden by the Show overlays preference, an unavailable selected monitor, or an overlay failure.
