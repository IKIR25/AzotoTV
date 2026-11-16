# Nearest classic (Switch) hairstyle for every Tomodachi Life hairstyle, by 3D shape.
import sys, json, numpy as np
sys.path.insert(0, sys.argv[1]); import bfres
from scipy.spatial import cKDTree
rows = json.load(open('MiiParts.json'))
def pts(fname):
    shapes, _ = bfres.model(f'Model/Mii{fname}.bfres.zs'); seen, out = set(), []
    for s in shapes:
        if s['name'] in seen: break
        seen.add(s['name']); out.append(s['p'])
    p = np.concatenate(out)
    # even out vertex density: keep one point per 1 cm voxel
    k = np.unique(np.round(p / 0.01).astype(int), axis=0, return_index=True)[1]
    return p[k]
hair = {r['PartsIndex']: r for r in rows if r['Category'] == 'Hair' and r['PartsIndex'] >= 0}
front = {r['PartsIndex']: r['FileName'] for r in rows if r['Category'] == 'HairFront' and r['PartsIndex'] >= 0}
legacy = {i: r for i, r in hair.items() if r['FileName'].startswith('HairAllLegacy')}
L = {}
for i, r in legacy.items():
    p = pts(r['FileName']); L[(i, 0)] = (p, cKDTree(p))
    if r['IsFlippable']:
        q = p * [-1, 1, 1]; L[(i, 1)] = (q, cKDTree(q))
def nearest(p):
    t = cKDTree(p); best = []
    for key, (q, tq) in L.items():
        d = tq.query(p)[0].mean() + t.query(q)[0].mean()
        best.append((d, key))
    best.sort(); return best
out = {}
F = {i: pts(n) for i, n in front.items()}
for i, r in sorted(hair.items()):
    n = r['FileName']
    if n.startswith('HairAllLegacy') or n == 'HairNothing': continue
    if n.startswith('HairAll'):
        b = nearest(pts(n)); out[i] = b[0][1]
        print(i, n, [(k, round(d, 3)) for d, k in b[:3]], flush=True)
    elif n.startswith('HairBack'):
        back = pts(n); row = {}
        for fi, fp in F.items():
            row[fi] = nearest(np.concatenate([back, fp]))[0][1]
        out[i] = row
        print(i, n, 'fronts ->', sorted(set(v for v in row.values())), flush=True)
json.dump({str(k): v for k, v in out.items()}, open('hairmatch.json', 'w'))
