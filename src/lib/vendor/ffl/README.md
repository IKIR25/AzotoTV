# FFL.js (not in the repo)

[FFL.js](https://github.com/ariankordi/FFL.js) 2.2.0 by Arian Kordi, commit
3a1236753eba69c9260bfc46f840079cff75019c (the one miijs 3.1.0 pins), licence
AGPL-3.0-only: draws Miis with the Wii U's own Mii library (FFL, compiled to
WebAssembly) and shaders.

`tools/fetch-mii-renderer.sh` puts it here (only the files the app uses), with
the rest of what drawing needs:

- The WebAssembly module, `static/mii/ffl-emscripten.js` (the package's
  `examples/ffl-emscripten-single-file.cjs`), loaded as a classic script: it
  sets `globalThis.ModuleFFL`.
- `static/mii/mii{Male,Female}Body.glb`, the bodies miijs ships.

The Mii models and textures are Nintendo's `FFLResHigh.dat` (Wii U system
title 0005001B10056000), which nothing fetches: mii.rs serves it from the
server's data folder.

Without these files the app builds all the same and the Mii app can't draw
(`$lib/mii/render.ts`). A build that includes FFL.js has to follow the AGPL;
the rest of the project is MIT.
