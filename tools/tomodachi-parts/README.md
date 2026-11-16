# Tomodachi Life parts -> Switch parts

Makes the tables in `src/lib/mii/tomodachi.ts` (how AzotoTV draws Tomodachi
Life: Living the Dream Miis). Run again if a game update adds parts.

Needs a dump of the game's romfs (from your own copy)
and Python with numpy, scipy, pillow, zstandard, texture2ddecoder.

```sh
mkdir work && cd work
cp -r <romfs>/Model/MiiHair* <romfs>/Model/MiiHead00.bfres.zs Model/
cp -r <romfs>/Icon/Face_* Icon/
cp <romfs>/RSDB/MiiParts.Product.100.rstbl.byml.zs . && zstd -d MiiParts*.zs
python -c "import json,byml; json.dump(byml.load(open('MiiParts.Product.100.rstbl.byml','rb').read()), open('MiiParts.json','w'))"
python hairmatch.py <this dir>   # -> hairmatch.json (slow: ~15 min, back hairs x 83 fringes)
python iconmatch.py <this dir>   # -> iconmatch.json
python hairview.py <this dir> 163 L89   # look at a match: game hair 163 vs classic 89
```

What was found (2026-11-08):
- `RSDB/MiiParts` lists every part: `PartsIndex` = the number in CharInfoEx,
  `FileName` = the model/icon. Classic Switch parts keep their number in the
  file name (`HairAllLegacy033` = hair 33, `Eye012`, `Mouth023`; beard,
  mustache and glasses count "Nothing" as 0 like the Switch). Eyebrows: a new
  "Nothing" took slot 23, classic eyebrow 23 moved to slot 24.
- Hair: `HairAll*` = whole styles, `HairBack*` = a back that takes one of 83
  fringes (`hairTypeFront`). Matched by 3D shape (Chamfer distance between the
  models' vertices, classic styles also mirrored).
- Other parts: by editor icon (`Icon/Face_*.bntx`, ASTC 5x5, block-linear).
- `faceFlags` bit 7 = the mole is on. Eyebrow X is the Switch's + 2. Skin
  colours: 100-109 = the Switch's 0-9, 110-113 new, under 100 a common colour.
- No favourite colour in CharInfoEx.
