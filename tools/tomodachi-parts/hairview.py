# Point-splat views (front, side, top) of Tomodachi hair models, side by side.
import sys, json, numpy as np
sys.path.insert(0, sys.argv[1]); import bfres
from PIL import Image, ImageDraw
rows = json.load(open('MiiParts.json'))
hair = {r['PartsIndex']: r['FileName'] for r in rows if r['Category'] == 'Hair'}
front = {r['PartsIndex']: r['FileName'] for r in rows if r['Category'] == 'HairFront'}
head = bfres.model('Model/MiiHead00.bfres.zs')[0][0]['p']
def pts(n):
    sh = bfres.model(f'Model/Mii{n}.bfres.zs')[0]; seen, out = set(), []
    for s in sh:
        if s['name'] in seen: break
        seen.add(s['name']); out.append(s['p'])
    return np.concatenate(out)
def view(p, axes, depth, S=200):
    im = Image.new('RGB', (S, S), 'white'); dr = ImageDraw.Draw(im)
    allp = [(head, 0), (p, 1)]
    pts_ = np.concatenate([np.c_[q, np.full(len(q), k)] for q, k in allp])
    d = pts_[:, depth[0]] * depth[1]
    order = np.argsort(d)
    lo, hi = d.min(), d.max()
    for i in order:
        x, y, k = pts_[i, axes[0]], pts_[i, axes[1]], pts_[i, 3]
        X, Y = (x + 0.55) * S / 1.1, (0.95 - y) * S / 1.1
        t = (d[i] - lo) / (hi - lo + 1e-9)
        col = (int(60 + 120 * t), int(35 + 80 * t), int(15 + 40 * t)) if k else (int(200 + 50 * t), int(170 + 50 * t), int(140 + 50 * t))
        dr.ellipse((X - 2, Y - 2, X + 2, Y + 2), fill=col)
    return im
specs = sys.argv[2:]  # e.g. 163 L89 165+2
tiles = []
for s in specs:
    if s.startswith('L'): p = pts(hair[int(s[1:].split('f')[0])]); flip = 'f' in s
    elif '+' in s: b, f = map(int, s.split('+')); p = np.concatenate([pts(hair[b]), pts(front[f])]); flip = False
    else: p = pts(hair[int(s)]); flip = False
    if flip: p = p * [-1, 1, 1]
    row = Image.new('RGB', (600, 215), 'white')
    for i, (ax, dp) in enumerate([((0, 1), (2, 1)), ((2, 1), (0, -1)), ((0, 2), (1, 1))]): row.paste(view(p, ax, dp), (i * 200, 0))
    ImageDraw.Draw(row).text((4, 200), s, fill='black'); tiles.append(row)
W = Image.new('RGB', (600 * 2, 215 * ((len(tiles) + 1) // 2)), 'white')
for i, t in enumerate(tiles): W.paste(t, ((i % 2) * 600, (i // 2) * 215))
W.save('hairview.png')
