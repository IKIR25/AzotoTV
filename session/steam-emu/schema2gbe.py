#!/usr/bin/env python3
"""Steam's cached UserGameStatsSchema_<appid>.bin -> gbe_fork achievements.json + stats.json (+ icons).

usage: schema2gbe.py <schema.bin> <steam_settings dir> [--no-icons]
"""
import json, os, struct, sys, urllib.request

def read_kv(buf, i=0):
    out = {}
    while True:
        t = buf[i]; i += 1
        if t == 0x08:
            return out, i
        end = buf.index(b'\0', i); key = buf[i:end].decode('utf-8', 'replace'); i = end + 1
        if t == 0x00:
            val, i = read_kv(buf, i)
        elif t == 0x01:
            end = buf.index(b'\0', i); val = buf[i:end].decode('utf-8', 'replace'); i = end + 1
        elif t in (0x02, 0x04, 0x06):
            val = struct.unpack_from('<i', buf, i)[0]; i += 4
        elif t == 0x03:
            val = struct.unpack_from('<f', buf, i)[0]; i += 4
        elif t == 0x07:
            val = struct.unpack_from('<Q', buf, i)[0]; i += 8
        else:
            raise ValueError(f'kv type {t:#x} at {i}')
        out[key] = val

def loc(d, key):
    v = d.get(key, '')
    if isinstance(v, dict):
        return v.get(os.environ.get('STEAM_LANGUAGE', 'english')) or v.get('english') or next(iter(v.values()), '')
    return v

def main():
    src, dst = sys.argv[1], sys.argv[2]
    icons = '--no-icons' not in sys.argv
    root, _ = read_kv(open(src, 'rb').read())
    (appid, game), = root.items()
    stats_out, ach_out = [], []
    for s in game.get('stats', {}).values():
        typ = s.get('type', 0)
        typ = {'INT': 1, 'FLOAT': 2, 'AVGRATE': 3, 'ACHIEVEMENTS': 4, 'GROUPACHIEVEMENTS': 5}.get(typ, typ)
        typ = int(typ)
        if typ == 4 or typ == 5:  # achievements (5 = grouped)
            for b in s.get('bits', {}).values():
                disp = b.get('display', {})
                a = {
                    'name': b['name'],
                    'displayName': loc(disp, 'name'),
                    'description': loc(disp, 'desc'),
                    'hidden': str(disp.get('hidden', 0)),
                    'icon': f"images/{disp.get('icon', '')}",
                    'icongray': f"images/{disp.get('icon_gray', '')}",
                }
                if 'progress' in b:
                    p = b['progress']
                    a['progress'] = {'value': p.get('value', {}).get('operand1', ''),
                                     'max_val': str(p.get('max_val', 0)),
                                     'min_val': str(p.get('min_val', 0))}
                ach_out.append(a)
        elif typ in (1, 2, 3):
            st = {1: 'int', 2: 'float', 3: 'avgrate'}[typ]
            ach = s.get('default', 0)
            stats_out.append({'name': s.get('name', ''), 'type': st,
                              'default': str(ach), 'global': '0'})
    os.makedirs(dst, exist_ok=True)
    json.dump(ach_out, open(f'{dst}/achievements.json', 'w'), indent=2, ensure_ascii=False)
    json.dump(stats_out, open(f'{dst}/stats.json', 'w'), indent=2, ensure_ascii=False)
    print(f'{appid}: {len(ach_out)} achievements, {len(stats_out)} stats')
    if icons:
        os.makedirs(f'{dst}/images', exist_ok=True)
        for a in ach_out:
            for k in ('icon', 'icongray'):
                f = a[k][len('images/'):]
                p = f'{dst}/images/{f}'
                if not f or os.path.exists(p):
                    continue
                url = f'https://cdn.akamai.steamstatic.com/steamcommunity/public/images/apps/{appid}/{f}'
                try:
                    urllib.request.urlretrieve(url, p)
                except Exception as e:
                    print('icon failed', f, e)

if __name__ == '__main__':
    main()
