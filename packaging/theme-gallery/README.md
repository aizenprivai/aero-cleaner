# AeroClean theme gallery

Themes made by the AeroClean community. AeroClean shows them under **Themes → 🌐 Gallery**: each card is a live preview drawn from the theme file, **👁 Preview** tries it on the whole app, and **⬇ Install** adds it to your themes.

Nothing is downloaded until you open the gallery, and every download is checked against this repository's `index.json` (size and SHA-256) before it's installed. Themes are data only: nothing in them runs.

## Share your theme

1. Make your theme in AeroClean's **Theme Studio** (Themes → 🎨 Customize), then click **📤 Export**. You get one `.aerotheme` file, with any background pictures inside it.
2. Fork this repository and add the file to `themes/`. Name it with lowercase letters, numbers and dashes, like `themes/ocean-breeze.aerotheme`.
3. Fill in `[meta]`: `name`, `author` (your name or nickname) and a short `description`.
4. Open a pull request. The check runs `aero-cleaner theme check` on every theme; fix anything it reports.

Please keep it friendly: no logos, trademarks or pictures you don't have the right to share, and pictures under 8 MB (smaller is better: most people download them).

When a pull request is merged, the index is rebuilt automatically.

## For maintainers

`index.json` is generated, don't edit it by hand:

```bash
aero-cleaner theme index themes > index.json
```

The workflow in `.github/workflows/check.yml` downloads the latest AeroClean AppImage for the checks, so it needs an AeroClean release with theme support (2.1 or newer).
