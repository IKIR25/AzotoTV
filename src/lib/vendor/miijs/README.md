# miijs (vendored)

The browser build of [miijs](https://github.com/Stewared/MiiJS) 3.1.0 (ISC
licence, by Stewared): every Mii format (Wii, DS, 3DS/Wii U, Switch, Studio,
amiibo, QR codes), conversions between consoles and per-console instructions.

Copied from the npm package's `dist/` rather than installed, because the
package pulls in native Node dependencies (canvas, sharp, jsdom, webgpu) the
TV app doesn't need. Left out: its renderer (`miiRendering` + three.js, which
needs the Wii U's FFLResHigh.dat) and the source maps; the renderer's dynamic
import in `miijs.browser.esm.js` is replaced by a rejected promise.

Use it through `$lib/mii/miijs.ts`, not directly.
