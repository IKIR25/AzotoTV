# Minimal BNTX reader: first texture, mip 0, ASTC or RGBA8, Tegra block-linear.
import struct, sys, zstandard, numpy as np, texture2ddecoder
from PIL import Image
ASTC = {0x2d: (4, 4), 0x2e: (5, 4), 0x2f: (5, 5), 0x30: (6, 5), 0x31: (6, 6), 0x32: (8, 5), 0x33: (8, 6), 0x34: (8, 8)}
def deswizzle(data, wb, hb, bpb, bh):
    gw = (wb * bpb + 63) // 64
    out = bytearray(wb * hb * bpb)
    for y in range(hb):
        for x in range(wb):
            xb = x * bpb
            gob = (y // (8 * bh)) * 512 * bh * gw + (xb // 64) * 512 * bh + (y % (8 * bh) // 8) * 512
            a = gob + ((xb % 64) // 32) * 256 + ((y % 8) // 2) * 64 + ((xb % 32) // 16) * 32 + (y % 2) * 16 + (xb % 16)
            o = (y * wb + x) * bpb
            out[o:o + bpb] = data[a:a + bpb]
    return bytes(out)
def load(path):
    d = open(path, 'rb').read()
    if path.endswith('.zs'): d = zstandard.ZstdDecompressor().decompress(d, max_output_size=1 << 28)
    t = d.find(b'BRTI')
    fmt = struct.unpack_from('<I', d, t + 0x1c)[0]
    w, h = struct.unpack_from('<II', d, t + 0x24)
    bhl = struct.unpack_from('<I', d, t + 0x34)[0]
    ptrs = struct.unpack_from('<Q', d, t + 0x70)[0]
    mip0 = struct.unpack_from('<Q', d, ptrs)[0]
    kind, typ = fmt >> 8, fmt & 0xff
    if kind in ASTC:
        bw, bhp = ASTC[kind]; wb, hb = -(-w // bw), -(-h // bhp)
        raw = deswizzle(d[mip0:], wb, hb, 16, 1 << bhl)
        rgba = texture2ddecoder.decode_astc(raw, w, h, bw, bhp)
        return Image.frombytes('RGBA', (w, h), rgba, 'raw', 'BGRA')
    if kind == 0x0b:
        raw = deswizzle(d[mip0:], w, h, 4, 1 << bhl)
        return Image.frombytes('RGBA', (w, h), raw)
    raise ValueError('format %x' % fmt)
if __name__ == '__main__':
    load(sys.argv[1]).save(sys.argv[2])
