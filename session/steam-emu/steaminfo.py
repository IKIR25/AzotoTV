#!/usr/bin/env python3
"""A library entry's info.json (Italian Steam store data) and art/{cover,hero,logo,header}.

usage: steaminfo.py <appid> <library entry>

Store descriptions are often clumsy or English: read info.json and fix it by hand.
"""
import html, json, os, re, sys, urllib.request

appid, entry = sys.argv[1], sys.argv[2]
UA = {'User-Agent': 'Mozilla/5.0'}


def get(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=30).read()


d = json.loads(get(f'https://store.steampowered.com/api/appdetails?appids={appid}&l=italian&cc=it'))[appid]['data']
desc = html.unescape(re.sub(r'<[^>]+>', ' ', d.get('short_description', ''))).strip()
desc = re.sub(r'\s+', ' ', desc)
trailers = []
for m in d.get('movies', []):
    url = m.get('hls_h264') or (m.get('mp4') or {}).get('max')
    if url:
        trailers.append({'name': m.get('name', 'Trailer'), 'url': url})
year = re.search(r'(19|20)\d\d', d.get('release_date', {}).get('date', ''))
info = {
    'description': desc,
    'genres': [g['description'] for g in d.get('genres', [])],
    'developers': d.get('developers', []),
    'year': year.group(0) if year else None,
    'trailers': trailers,
}
json.dump(info, open(f'{entry}/info.json', 'w'), indent=1, ensure_ascii=False)
os.makedirs(f'{entry}/art', exist_ok=True)
base = f'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{appid}'
for name, kind in [('library_600x900.jpg', 'cover.jpg'), ('library_hero.jpg', 'hero.jpg'), ('logo.png', 'logo.png'), ('header.jpg', 'header.jpg')]:
    try:
        open(f'{entry}/art/{kind}', 'wb').write(get(f'{base}/{name}'))
    except Exception as e:
        print(appid, name, e)
print(appid, d['name'], '|', len(trailers), 'trailers |', desc)
print('   ', info['genres'], info['developers'], info['year'], [t['name'] for t in trailers])
