# Minimal Switch BFRES (v10) reader: vertex positions of every FVTX.
import struct, sys, zstandard, numpy as np
def u64(d, o): return struct.unpack_from('<Q', d, o)[0]
def sstr(d, o):
    n = struct.unpack_from('<H', d, o)[0]; return d[o+2:o+2+n].decode()
def load(path):
    d = open(path, 'rb').read()
    if path.endswith('.zs'): d = zstandard.ZstdDecompressor().decompress(d, max_output_size=1 << 28)
    assert d[:4] == b'FRES'
    ext = d[0xee]
    buf_base = struct.unpack_from('<q', d, u64(d, 0xb0) + 8)[0]
    out = []
    o = d.find(b'FVTX')
    while o >= 0:
        attr_off = u64(d, o+8)
        size_off, stride_off = u64(d, o+0x30), u64(d, o+0x38)
        boff, nattr, nbuf, idx, nv, skin, align = struct.unpack_from('<iBBHIHH', d, o+0x48)
        attrs = {}
        for i in range(nattr):
            a = attr_off + 0x10*i
            name = sstr(d, u64(d, a)); fmt = struct.unpack_from('>H', d, a+8)[0]
            aoff, bidx = struct.unpack_from('<HH', d, a+12)
            attrs[name] = (fmt, aoff, bidx)
        bufs, p = [], buf_base + boff
        for b in range(nbuf):
            size = struct.unpack_from('<I', d, size_off + 0x10*b)[0]
            stride = struct.unpack_from('<I', d, stride_off + 0x10*b)[0]
            if align: p = (p + align - 1) // align * align
            bufs.append((d[p:p+size], stride)); p += size
        out.append(dict(nv=nv, skin=skin, attrs=attrs, bufs=bufs, ext=ext))
        o = d.find(b'FVTX', o+4)
    return out
def positions(path, first=True):
    pts = []
    for v in load(path):
        fmt, aoff, bidx = v['attrs']['_p0']
        data, stride = v['bufs'][bidx]
        if fmt == 0x518:   # float32 x3
            arr = np.frombuffer(data, dtype=np.uint8).reshape(-1, stride)[:, aoff:aoff+12].copy().view('<f4')
        elif fmt == 0x2105 or fmt == 0x1d05 or fmt == 0x1505:
            arr = np.frombuffer(data, dtype=np.uint8).reshape(-1, stride)[:, aoff:aoff+8].copy().view('<f2').astype(np.float32)[:, :3]
        else:
            raise ValueError('fmt %x' % fmt)
        pts.append(arr[:v['nv']])
        if first: break
    return np.concatenate(pts)
if __name__ == '__main__':
    for f in sys.argv[1:]:
        for v in load(f):
            print(f.split('/')[-1], 'nv', v['nv'], 'skin', v['skin'], 'ext', hex(v['ext']), {k: (hex(a[0]), a[1], a[2]) for k, a in v['attrs'].items()}, [(len(b), s) for b, s in v['bufs']])

def quat_mat(x, y, z, w):
    return np.array([[1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w)], [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w)], [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y)]])
def euler_mat(x, y, z):
    cx, sx, cy, sy, cz, sz = np.cos(x), np.sin(x), np.cos(y), np.sin(y), np.cos(z), np.sin(z)
    rx = np.array([[1, 0, 0], [0, cx, -sx], [0, sx, cx]]); ry = np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]); rz = np.array([[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]])
    return rz @ ry @ rx
def model(path):
    """Shapes of the first model: name, bone, skin count, world-space positions."""
    d = open(path, 'rb').read()
    if path.endswith('.zs'): d = zstandard.ZstdDecompressor().decompress(d, max_output_size=1 << 28)
    vtx = load(path)
    o = d.find(b'FSKL'); bones = []
    if o >= 0:
        arr = u64(d, o + 0x10); n = struct.unpack_from('<H', d, o + 0x38)[0]
        for i in range(n):
            b = arr + 0x58 * i
            name = sstr(d, u64(d, b)); parent = struct.unpack_from('<h', d, b + 0x22)[0]; flags = struct.unpack_from('<I', d, b + 0x2c)[0]
            sc = np.array(struct.unpack_from('<3f', d, b + 0x30)); rot = struct.unpack_from('<4f', d, b + 0x3c); pos = np.array(struct.unpack_from('<3f', d, b + 0x4c))
            R = euler_mat(*rot[:3]) if flags & 0x1000 else quat_mat(*rot)
            M = np.eye(4); M[:3, :3] = R * sc; M[:3, 3] = pos
            if parent >= 0: M = bones[parent][2] @ M
            bones.append((name, parent, M))
        nsm, nrig = struct.unpack_from('<HH', d, o + 0x3a)
        mtx2bone = struct.unpack_from('<%dH' % (nsm + nrig), d, u64(d, o + 0x18)) if nsm + nrig else ()
    shapes = []
    o = d.find(b'FSHP')
    while o >= 0:
        name = sstr(d, u64(d, o + 8)); bone, vbi = struct.unpack_from('<HH', d, o + 0x54); skin = d[o + 0x5a]
        v = vtx[vbi]; fmt, aoff, bidx = v['attrs']['_p0']; data, stride = v['bufs'][bidx]
        raw = np.frombuffer(data, dtype=np.uint8).reshape(-1, stride)[:v['nv']]
        if fmt == 0x518: p = raw[:, aoff:aoff + 12].copy().view('<f4').reshape(-1, 3).astype(np.float64)
        elif fmt == 0x515: p = raw[:, aoff:aoff + 8].copy().view('<f2').reshape(-1, 4)[:, :3].astype(np.float64)
        else: raise ValueError('position format %x' % fmt)
        if skin == 0 and bones:
            M = bones[bone][2]; p = p @ M[:3, :3].T + M[:3, 3]
        elif skin == 1 and bones:
            ifmt, ioff, ib = v['attrs']['_i0']; idata, istride = v['bufs'][ib]
            idx = np.frombuffer(idata, dtype=np.uint8).reshape(-1, istride)[:v['nv'], ioff]
            for m in np.unique(idx):
                M = bones[mtx2bone[m]][2]; sel = idx == m; p[sel] = p[sel] @ M[:3, :3].T + M[:3, 3]
        shapes.append(dict(name=name, bone=bones[bone][0] if bones else None, skin=skin, p=p))
        o = d.find(b'FSHP', o + 4)
    return shapes, bones
