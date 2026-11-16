#!/usr/bin/env python3
"""The DLCs installed for a game in the PC's Steam -> gbe_fork's configs.app.ini.

usage: dlcs.py <appmanifest_<appid>.acf> <out dir (the achievements dir given to install.sh)>

Only the DLCs Steam installed (the ones bought) are reported as owned; names
come from the Steam store.
"""
import json, re, sys, urllib.request

acf, out = sys.argv[1], sys.argv[2]
ids = sorted(set(re.findall(r'"dlcappid"\s+"(\d+)"', open(acf).read())), key=int)
names = {}
for i in ids:
    try:
        url = f'https://store.steampowered.com/api/appdetails?appids={i}&filters=basic'
        d = json.loads(urllib.request.urlopen(urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'}), timeout=30).read())
        names[i] = d[i]['data']['name']
    except Exception:
        names[i] = f'DLC {i}'
with open(f'{out}/configs.app.ini', 'w') as f:
    f.write('[app::dlcs]\n# only these are owned (some games test a fake DLC to spot emulators)\nunlock_all=0\n')
    for i in ids:
        f.write(f'{i}={names[i]}\n')
print(f'{len(ids)} DLCs:', ', '.join(names[i] for i in ids))
