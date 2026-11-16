import struct, sys, json
def load(d):
    assert d[:2]==b'YB'
    ver,koff,soff,root=struct.unpack_from('<HIII',d,2)
    def strtab(off):
        if not off: return []
        n=struct.unpack_from('<I',d,off)[0]>>8
        offs=struct.unpack_from('<%dI'%(n+1),d,off+4)
        return [d[off+o:d.index(b'\0',off+o)].decode() for o in offs[:n]]
    keys=strtab(koff); strs=strtab(soff)
    def val(t,v):
        if t==0xA0: return strs[v]
        if t in (0xC0,0xC1): return node(v)
        if t==0xD0: return bool(v)
        if t==0xD1: return struct.unpack('<i',struct.pack('<I',v))[0]
        if t==0xD2: return struct.unpack('<f',struct.pack('<I',v))[0]
        if t==0xD3: return v
        if t in (0xD4,0xD5,0xD6):
            raw=d[v:v+8]; return struct.unpack({0xD4:'<q',0xD5:'<Q',0xD6:'<d'}[t],raw)[0]
        if t==0xFF: return None
        return ('?%x'%t,v)
    def node(off):
        t=d[off]; n=struct.unpack_from('<I',d,off)[0]>>8
        if t==0xC0:
            types=d[off+4:off+4+n]; base=off+4+((n+3)&~3)
            return [val(types[i],struct.unpack_from('<I',d,base+4*i)[0]) for i in range(n)]
        if t==0xC1:
            out={}
            for i in range(n):
                e=off+4+8*i; k=struct.unpack_from('<I',d,e)[0]&0xffffff; tt=d[e+3]; v=struct.unpack_from('<I',d,e+4)[0]
                out[keys[k]]=val(tt,v)
            return out
        return ('?node%x'%t,off)
    return node(root) if root else None
if __name__=='__main__':
    for f in sys.argv[1:]:
        print('==',f.split('/')[-1]); print(json.dumps(load(open(f,'rb').read())))
