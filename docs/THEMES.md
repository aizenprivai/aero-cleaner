# Making AeroClean themes

A theme is a small text file ending in `.aerotheme`. It describes everything about AeroClean's look: colors, how glossy and round things are, the font, animation, and the background as a stack of layers. Themes are data only: nothing in them runs.

You can make one without touching a file: on the **Themes** screen, pick a theme and click **🎨 Customize** to open the **Theme Studio**. This guide is for when you want to edit the file yourself, or understand what the Studio saves.

## The shortest theme

A theme starts from a **base** (Frutiger Aero if you don't name one) and only lists what it changes:

```toml
format = 1

[meta]
name = "Mint"

[colors]
accent = "#3cb371"
```

Save it in AeroClean's themes folder (Theme Studio → **📂 Themes folder**), or run `aero-cleaner theme apply mint.aerotheme`. While AeroClean is open, it notices when you save the file and updates right away. `aero-cleaner theme check mint.aerotheme` explains any mistake.

The built-in themes are good starting points: they're in [`themes/`](../themes/).

## `[meta]`

| Setting | What it is |
|---|---|
| `name` | Required. Up to 60 characters. |
| `description` | One short line, shown on the theme's card. |
| `author` | You. |
| `base` | The theme to start from: any built-in theme or one of yours, by name. |

## `[colors]`

Colors are written `"#rrggbb"` (or `"#rrggbbaa"` with transparency).

| Setting | Used for |
|---|---|
| `text` | Main text. Keep a contrast of 4.5 or more with the glass (the Studio shows it). |
| `soft` | Secondary text. Aim for 3 or more. |
| `glass` | The tint of the frosted (or smoked) glass panels. |
| `accent` | Selected items, outlines, blue buttons. |
| `go` | Main buttons like **Clean now**, check marks. |
| `user_orb` | Icons of things in your home folder. |
| `root_orb` | Icons of system items (that need a password). |
| `logo` | The AeroClean logo. |
| `bubble` | Bubbles, in the scene and in the celebration after a clean. |

## `[material]`

| Setting | Range | What it does |
|---|---|---|
| `dark_glass` | `true` / `false` | Dark smoked glass instead of light frosted glass. |
| `corners` | 0 to 2 | Roundness (1 is the classic look, 0 is square). |
| `gloss` | 0 to 1.5 | The white shine on buttons, orbs and panels. |
| `glass` | 0.3 to 1.5 | How opaque the glass is. |
| `buttons` | `"aqua"`, `"soft"`, `"flat"` | Glossy pills, a plain gradient, or one color. |

## `[font]`

| Setting | Range | What it does |
|---|---|---|
| `family` | `"system"`, `"builtin"` | Your system's UI font, or the font built into AeroClean. |
| `size` | 0.8 to 1.3 | Text size. (For bigger text everywhere, use Settings → Text and controls size.) |

## `[motion]`

| Setting | Range | What it does |
|---|---|---|
| `speed` | 0 to 3 | Speed of every animation. |
| `transition` | `"none"`, `"fade"`, `"slide"` | How a screen appears when you switch to it. |

**Reduce motion** in Settings always turns animations and transitions off, whatever the theme says.

## `[sound]`

`pack` and `volume` (0 to 1) are read but not used yet: sound packs come in a later version.

## The scene: `[[scene.layers]]`

The background is a list of layers, drawn top to bottom (the first one is at the back). Listing layers replaces the base theme's layers entirely.

Every layer has:

| Setting | Range | Default |
|---|---|---|
| `type` | see below | required |
| `opacity` | 0 to 1 | 1 |
| `speed` | 0 to 5 | 1 (multiplies the theme's speed) |
| `enabled` | `true` / `false` | `true` |

Positions (`x`, `y`) are fractions of the window: `x = 0` is the left edge, `y = 0` the top.

| `type` | Its own settings |
|---|---|
| `sky` | `stops`: 2 to 16 colors from top to bottom, like `[[0.0, "#0a58b4"], [1.0, "#ebfaff"]]` |
| `sun`, `moon` | `x`, `y`, `color` |
| `rays` | `strength` (0 to 255, default 45); `x`, `y` (default: the sun's or moon's position) |
| `stars` | `count` (up to 400, default 90) |
| `aurora` | none |
| `hills` | `back` and `front`: color stops, like the sky's |
| `bubbles` | `count` (up to 80, default 18), `color` (default: `colors.bubble`) |
| `clouds` | `count` (up to 20), `y` (height, default 0.22), `size` (0.2 to 3), `color` |
| `fish` | `count` (up to 30), `y` (default 0.62), `color` |
| `fireflies` | `count` (up to 150), `color` |
| `petals` | `count` (up to 150), `color` |
| `snow` | `count` (up to 300), `color` |
| `leaves` | `count` (up to 120), `color` |
| `picture` | `file`: a `.png`, `.jpg` or `.webp` next to the theme file, in your home folder (`~/Pictures/sky.jpg`), or a full path; `frosted` (default `true`) |

Busy scenes are fine: AeroClean draws fewer particles on battery when a scene gets heavy (the Studio shows a scene weight meter).

A full example:

```toml
format = 1

[meta]
name = "Night Pond"
base = "Aurora Night"
description = "Fireflies over a quiet pond"

[colors]
accent = "#3d8f5a"

[[scene.layers]]
type = "sky"
stops = [[0.0, "#050816"], [1.0, "#24304f"]]

[[scene.layers]]
type = "moon"
x = 0.8
y = 0.15
color = "#fff3d6"

[[scene.layers]]
type = "fish"
color = "#ffb347"
count = 5
y = 0.75
opacity = 0.6

[[scene.layers]]
type = "fireflies"
count = 60
speed = 0.7
```

## Sharing

- **Theme Studio → 📤 Export** (or `aero-cleaner theme export "Night Pond"`) saves one file with the complete theme. Pictures go inside it, in a `[files]` section, so it works on any computer.
- **Themes → 📥 Import**, dragging the file onto AeroClean's window, or `aero-cleaner theme apply file.aerotheme` adds a shared theme. Pictures are unpacked next to it in your themes folder. Files are checked first: only plain picture files, nothing that runs, nothing written outside the themes folder.
- **Themes → 🌐 Gallery** lists themes shared by the community. To share yours there, see the gallery's README: export your theme and open a pull request.

## Themes from a picture

**Themes → 📥 Import → 🖼 Themes from a picture** opens a wallpaper or photo (`.png`, `.jpg` or `.webp`). AeroClean finds its main colors and how they run from top to bottom, then makes three themes: **Light**, **Dark** and **Vivid**. Text colors are adjusted until they pass the readability check above.

Tick **Use the picture as the background** to put the picture itself behind the frosted glass (dimmed in the dark theme). Saving copies a smaller JPEG of it into your themes folder, next to the theme, so the theme keeps working if you move the original.

The themes are ordinary files: open them in the Theme Studio to change anything.

## Day and night

**Themes → 🌗 Day & night** picks a day theme and a night theme, any of the built-in ones or yours. AeroClean switches between them:

- **With your system's dark mode**, when your system tells apps about it (GNOME, KDE and others on Linux, Windows, macOS).
- **On a schedule**, at the times you choose.
- **At sunrise and sunset**, worked out from your time zone's main city. No location and no internet are needed; it's usually within a few minutes for that city.

The switch is a crossfade: colors blend and the new scene fades in over the old one. A theme you pick by hand stays until the next sunrise or sunset (or schedule change). **Reduce motion** switches instantly.

## Command line

```bash
aero-cleaner theme list
aero-cleaner theme check my-theme.aerotheme
aero-cleaner theme apply my-theme.aerotheme
aero-cleaner theme export "My Theme" my-theme.aerotheme
aero-cleaner theme index folder-of-themes > index.json
```
