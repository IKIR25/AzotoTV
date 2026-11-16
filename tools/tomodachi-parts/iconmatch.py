# Nearest classic part for every Tomodachi-only face part, by editor icon.
import sys, json, os, numpy as np
sys.path.insert(0, sys.argv[1]); import bntx
from PIL import Image
rows = json.load(open('MiiParts.json'))
import re
# Switch part counts. Classic parts keep the Switch number in their file name
# (EyeNNN, MouthNNN...); "Nothing" first (index = Switch id) for beard, mustache.
COUNT = {'Eye': 60, 'Eyebrow': 24, 'Nose': 18, 'Mouth': 36, 'Faceline': 12, 'Mustache': 6, 'Beard': 6}
EXTRA = {'BeardShort': 'Beard'}
def switch_id(cat, r):
    if cat in ('Mustache', 'Beard'):
        return r['PartsIndex'] if r['PartsIndex'] < COUNT[cat] else None
    m = re.fullmatch(cat + r'(\d+)', r['FileName'])
    return int(m[1]) if m and int(m[1]) < COUNT[cat] else None
def icon(r):
    n = r['EditorIconName'].removeprefix('MiiEditor_').removesuffix('_Uit')
    p = f'Icon/{n}.bntx.zs'
    if not n or not os.path.exists(p): return None
    im = bntx.load(p).convert('RGBA').resize((96, 96), Image.LANCZOS)
    a = np.asarray(im, dtype=np.float32) / 255
    return np.concatenate([a[..., :3] * a[..., 3:], a[..., 3:]], -1)
by = {}
for r in rows:
    if r['PartsIndex'] >= 0: by.setdefault(r['Category'], {})[r['PartsIndex']] = r
out = {}
for cat in list(COUNT) + list(EXTRA):
    ref = EXTRA.get(cat, cat)
    classic = {i: switch_id(ref, r) for i, r in by[ref].items()}
    known = {sid: icon(by[ref][i]) for i, sid in classic.items() if sid is not None}
    known = {i: v for i, v in known.items() if v is not None}
    m = {}
    for i, r in sorted(by[cat].items()):
        sid = None if cat in EXTRA else classic[i]
        if sid is not None:
            if sid != i: m[i] = sid
            continue
        v = icon(r)
        if v is None or r['FileName'].endswith('Nothing'): print(cat, i, r['FileName'], 'none'); m[i] = None; continue
        d = sorted((float(((v - k) ** 2).mean()), j) for j, k in known.items())
        m[i] = d[0][1]
        print(cat, i, r['FileName'], '->', [(j, round(x, 4)) for x, j in d[:3]])
    out[cat] = m
json.dump(out, open('iconmatch.json', 'w'))
