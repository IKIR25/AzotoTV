// Tomodachi Life: Living the Dream's Miis (CharInfoEx, 0x98 bytes) as Switch
// CharInfo (0x58), so AzotoTV can draw them. The game keeps every classic
// Switch part under its own number (hair 0-131, eyes 0-59...; the file names
// say so: HairAllLegacy033 = hair 33) and adds its own after them. Those new
// parts become the closest classic one: hairstyles by comparing the game's 3D
// models, the rest by its editor icons (tools/tomodachi-parts, 2026-11-08: the
// game's romfs MiiParts table + Model/MiiHair*.bfres + Icon/Face_*.bntx).
// Parts the Switch hasn't got at all (eyelashes, eyelids, highlights, short
// beards, wrinkles and make-up, which the game splits in two) are left out.
import { COMMON, SKIN } from './colors';

/** Field offsets in CharInfoEx (layout: ltd-save-editor's charInfoEx.ts). */
const EX = {
  name: 0x10,
  fontRegion: 0x26,
  gender: 0x27,
  height: 0x28,
  build: 0x29,
  regionMove: 0x2a,
  faceFlags: 0x2b,
  facelineType: 0x2c,
  facelineColor: 0x2d,
  hairType: 0x44,
  hairColor0: 0x46,
  hairTypeFront: 0x48,
  eyeType: 0x4e,
  eyeColor: 0x4f,
  eyeScale: 0x50,
  eyeAspect: 0x51,
  eyeRotate: 0x52,
  eyeX: 0x53,
  eyeY: 0x54,
  eyebrowType: 0x74,
  eyebrowColor: 0x75,
  eyebrowScale: 0x76,
  eyebrowAspect: 0x77,
  eyebrowRotate: 0x78,
  eyebrowX: 0x79,
  eyebrowY: 0x7a,
  noseType: 0x7b,
  noseScale: 0x7c,
  noseY: 0x7d,
  mouthType: 0x7e,
  mouthColor: 0x7f,
  mouthScale: 0x80,
  mouthAspect: 0x81,
  mouthY: 0x83,
  beardType: 0x84,
  beardColor: 0x85,
  mustacheType: 0x88,
  mustacheScale: 0x8a,
  mustacheY: 0x8c,
  glassType: 0x8d,
  glassColor: 0x8e,
  glassScale: 0x8f,
  glassY: 0x91,
  moleScale: 0x94,
  moleX: 0x95,
  moleY: 0x96,
} as const;
export const EX_LEN = 0x98;
const MOLE_ON = 0x80; // faceFlags bit 7 (ltd-save-editor calls it moleHigh: it's on/off)

/** Full hairstyles (HairAll*): Switch hair, +132 = mirrored. */
// prettier-ignore
const HAIR: Record<number, number> = {144:45,145:75,146:75,147:54,148:52,149:47,150:82,151:26,152:78,153:24,154:92,155:54,156:45,157:41,158:119,159:54,160:26,161:117,162:71,163:89,164:255,165:124,166:49,167:179,168:123,169:52,170:23,180:83,207:52,208:33,209:45,210:54,211:42,212:82,213:119,214:80,215:93,216:80,217:97,218:32,219:18,220:54,221:127,222:125,223:21,224:89,225:76,226:75,227:54,228:127,229:47,230:127,231:33,232:86,233:47,234:26,235:45,236:70,237:73,238:127,239:119,240:119,241:119,242:18,243:67,244:54,248:54,249:97,250:205,254:80,255:179,256:75,257:125,258:93,259:93,260:4,261:75,262:52,263:40,264:54,265:41,266:49,267:205,268:79,269:179,270:79,271:127,272:45,273:179,274:127,275:33,276:127,277:127,278:33,279:89,280:54,313:86,314:54,315:21,316:75,317:45,318:70,319:47,320:4,321:119,327:80,329:80,330:80,331:89,333:45,334:42,335:21,338:33,339:47,340:80};
/** Back hairstyles (HairBack*) go with a fringe (HairFront 0-82): per back,
 * the Switch hairs it can become and, per fringe, which one (a character). */
// prettier-ignore
const HAIR_BACK: Record<number, [number[], string]> = {
  132: [[3,10,12,15,16,17,20,32,80,82,93,101,117,128,129,148,234,260], '4e44fff14448f11844h184g93f418f44d31a1e46551f4df66654619b081f434714f08f4cf8848829166'],
  133: [[25,33,35,44,47,48,69,82,86,89,108,113,179,201], '20b21bb6b118b1394b038b1a01ba92120a1110b1116bbab14c11537039d11a113119891c899199101c6'],
  134: [[5,10,21,24,53,72,74,80,82,106,116,238], '2522222a222722b7222b72282226722226a29522224226227399098597122b22122772232772779222b'],
  135: [[6,12,16,32,82,89,129,131,144,148], '86118881111918851188218088111118111118888188118188111841128817138118881815888518118'],
  136: [[4,7,10,12,16,17,24,32,44,50,72,73,80,82,87,93,95,102,119,128,129,131,193,205,225], '0k377iiei77c752c4ik2c7ido7igc777jlefekii5hbiigii66ii98daec27ilf71i76cmi66c67ccelf6n'],
  137: [[5,21,44,53,72,74,82,89,106,116,122,238,248], '1411111911171187711b71111115711115911111113115117718026187c11b11311777171771778111a'],
  138: [[21,42,53,72,80,82,106,238], '03000006000400644007400000004000006000000020060044060653647001007004400404404460007'],
  139: [[3,4,7,80,82,93,123,225], '11531175515315533170311471153115555551733507151133555741530156572113335333333355537'],
  140: [[6,12,16,17,32,80,82,95,102,117,131,144,148,193,227,234], 'c71cbbb1111511b521e151f07c115cc277111ebb381b17b159112b6e15bc1a14b1c55d19c55c551311b'],
  141: [[10,11,12,20,21,22,80,82,95,98,108,129,230], '6b961cc09196110669556c474cc461914a041496110198c466191074960195110916619666666624163'],
  142: [[5,6,22,25,32,69,82,89,94,95,124,137,144,201], '73877880888778c7783878818880778780070387785889887788086a87d88284b887778777777703070'],
  143: [[25,33,37,40,43,44,47,48,52,70,82,86,108,179], '10111219211b115b6205b21c3111b111011b101112511c11661175a05b511c115114bb1dbbb1bb7b1d8'],
  206: [[25,33,37,40,43,44,47,48,52,70,82,86,108,179], '10111219221b235b6208b22c3223b1210383601832712c21661175a07b811c335114bb1dbbb1bb736d8'],
  245: [[25,33,37,40,44,47,48,52,70,82,86,108,179], '10111218211a214a5207a21b3113a1110513501132411b11554164904a411b11411caa1caaa1aa635c7'],
  246: [[3,4,7,72,80,82,93,96,129,131,225], '486441a71114446444aa4115a1164414166641a4440a161144a66a5364046961211444644444446a441'],
  247: [[3,4,7,10,21,42,80,82,93,225], '61666666868666366690664796186616886861966609181166888071063685692166666666666689666'],
  251: [[6,10,12,17,20,21,53,72,74,80,82,95,116,227,238], '9729555c5559531995de9550555895555885275955655b55993243a729155d551559995999999925994'],
  252: [[10,12,16,17,20,72,80,82,95,102,105,131,227,234], '6536693a9696633669c36d97869366d688363596690398996639437516069b320966669666666613664'],
  253: [[5,6,17,21,53,72,74,80,82,95,102,106,116,131,203,238], '75b7333c333732f7735f733133a673335663253733433933772b0285b7f3ad33e3377737777777b367f'],
  281: [[5,21,32,35,44,53,58,69,72,74,82,89,106,108,116,122,124,201,206,248], '38f331fef3fb36cbb1ijb31d13f9b33319e1e8ff337ff9f1bbff04agfbh3fc12513bbbfbbbb3bbf1eff'],
  282: [[4,7,10,12,16,17,20,21,24,32,80,82,93,95,102,128,129,131,148,193,225,227,234], '0g300mkee00a052a4el2ammb70eda00cfd2c5ge55e2iede7a85e65b03a20eh49170aaj38aaa0aa37cc6'],
  283: [[10,12,16,20,21,22,42,72,80,82,95,128,129,144,148,227], '8c18edd11228810881f582494e1a88e2ba141cd8820d1ad188113197180e162e02e8881888888814183'],
  284: [[5,6,10,12,16,17,20,32,80,82,94,95,144,148,227,234], '8e38dcd03448d52883e284f1bd338dd4b2242e4d552d3bda8854029e382d4e4724d8884888888835346'],
  285: [[3,4,7,27,42,80,82,93,96,225], '51751198117510755190511991175117170771955709171155977061053174792115557555555579558'],
  286: [[6,10,17,21,71,72,74,80,82,95,102,106,131,203,227,238], '7537333b333772b773ef7330333977335963253773433933772a448547133c33d33777377777774377d'],
  287: [[3,4,10,12,15,16,20,21,32,80,82,93,95,128,129,144,148], '9e391fg23159922993a29g7a713b9919db2b2eg9950g3c17990562a1092954582g1999399999993b996'],
  288: [[5,6,12,25,32,33,44,47,69,81,82,89,94,95,99,113,124,131,201,227], 'b32b5ff0f44b4e6bbfjebf5134feb444dd9eejcbb5844d4cb76406ag6bi4che4654bbb4bbbbbbb239b0'],
  289: [[6,10,12,16,20,32,80,82,95,101,129,144,148,227], '6a163b312336611662d163301321663381131d14631121c1661341791613111513c6663666666611111'],
  290: [[10,21,42,53,72,77,80,82,106,238], '64666668161666866619611716116616116161166631111166115674860612610166666666666681661'],
  291: [[10,12,17,20,21,22,27,72,74,80,82,95,102,105,128,129,131,227], '9f19942d4449922994h5944a49cb9949eb8427c99462cb44992132a719091g440449991999999914993'],
  292: [[10,21,28,72,80,82,93,111,116,123,131,238], '4344444814444404443b44151474444461244314442119414477775374047a440444444444444471447'],
  293: [[25,32,33,35,44,48,69,82,89,108,113,119,122,124,125,201], '80a88aa6a82888488a0e8ab908a88838098880c888faa9ab884b547dc8f8ce814b8888a8888888c888c'],
  294: [[0,10,11,82,89,90,117,131,193], '55555550555255140551255555504555500505552515505566550035521557551556285624254405000'],
  295: [[10,12,15,16,17,20,21,27,80,82,95,101,102,105,128,129,144,148,193,227,234], '8f18hg5d1338h00851j083k965cd8h53ea060fh54c7h1ac68541509b1805523h03h88i1888888816555'],
  296: [[21,25,35,44,69,74,82,89,108,113,118,122,124,201,238], '2192299492272ab7791e79980298722258b7a1b72249989b77bb436cb7d2b822d9277797777277b177b'],
  297: [[11,25,32,35,44,50,69,89,108,113,118,201,230], '3193399693373047791a7998139873331830a1933359989caa33341197b33832493a333a37737731336'],
  299: [[6,10,12,16,17,20,21,22,32,71,80,82,85,95,102,105,131,148,193,217,234], '3c233khfe33ah44a3eg7a3kgghegah33dg464je44e93egek004254bc2a13eg3813haai2a4aa3aa2g235'],
  300: [[25,33,44,48,56,69,79,82,89,108,113,115,125,130,201,247], '80881a15afb88828880c8abd9f1488b80c288018815bf9b1882b327628e8bc8b21b8881888888838886'],
  301: [[3,4,5,10,21,24,72,80,82,93,95,106,116,131,225,238], '16b744eb441714f774ef74484149711419c9b64744044a4475bb2086b734bd4134477745777777b499f'],
  302: [[25,33,44,56,69,79,82,89,108,115,125,188,201], '701711141197772771aa7118811377170ab7301771411a117711526527c19a792117771777777717775'],
  303: [[22,31,32,33,36,37,43,44,48,56,70,82,84,86,89,107,115,119,130,201,216,247], '3c3335ha5lgd517e95k0d5hiilhc1lggcc111khh118g5cgh147h87bc81j3gi327hf611311e131d8i118'],
  304: [[13,25,32,47,50,82,86,89,99,101,117,131,231], '21822c88822628878818620182088222888881ccc8488820c308885982c28b82c82a228ac782c688880'],
  305: [[10,11,25,50,82,89,98,108,230], '12611886666111111621188226611161271112611138678811161142610167110611116115111161118'],
  306: [[6,21,53,72,74,80,82,106,116,238], '13111118111511955119511011115111142121121121191122172263252119112112511215515571122'],
  307: [[25,33,37,40,44,47,48,52,86,89,108,179], '10111211211811485207821a311591110553501b32611a115b1174a068711a11411b851b89b188185b7'],
  308: [[29,112], '11111110111111011110111111111111110111111101101111111111110110110111111111111111111'],
  309: [[25,32,33,37,40,44,47,48,52,70,82,86,89,108,119,124,179], '20e223e9322b245c6308b3ed02edc2220d2420ee435e2d2e66ee75af5b522d215e2gbb2gbcb2bb7d26e'],
  310: [[5,10,21,53,72,80,82,106,116,238], '24222228222522755229522222225222228222222232222255220762751229221225522525525572222'],
  311: [[3,4,7,10,27,42,72,80,82,93,100,110,128,129,225], '7d777774a797773777e377a8e7a97717c9497da77740a91a77094486b747957e2a7777777777774e77b'],
  312: [[6,17,82,89,102,227,234], '15111611444111131411166011411161411116111111414111141121111141111611111113111111111'],
  322: [[5,6,10,11,21,32,35,58,72,80,82,94,98,116,118,124,137,248], '68b63bbdbbb967h99b4h9b414cbd93664dd4d4bb34gbbdbbeebb0bafb92bbd352b3993be399399d7000'],
  323: [[6,10,12,17,20,21,72,80,82,95,102,129,227,234], '3b333d31aaa733373ac17dd053a3735393333c333313a9d3333243862713a9331d37733337737723334'],
  324: [[10,12,16,17,20,80,82,95,101,102,105,117,128,129,131,148,193,227,234], '3d33393a9995330539h05i963f935fi3c3033d9333039799bb39436815039e32093b5g3b353355a3334'],
  325: [[5,10,12,17,21,32,53,72,74,80,82,95,102,116,131,234,238], '47334f3dc4c943393c7g94fa44c3944378d437c33363cbc4333c03a73913ce351449933339939924330'],
  326: [[9,11,19,42,50,72,80,82,84,98,125,130,151,216,230], '656666609696662666ba66ebb696669680066d96664198ee66c22c7526069361c96666666666662b666'],
  328: [[9,32,35,50,82,84,97,98,130,216,230], '6966666076766006668066a88670662650000906660770a066073045060600610766666666666600660'],
  336: [[28,111], '01000110111000001100011001100010100001110000101100111101100010000100001000000010001'],
  337: [[10,12,17,21,22,24,32,44,53,62,72,80,82,87,95,99,102,119,128,129,131,193], '6jh66hhdh6hb620bhhj4bhhca6heb666ik031jhh6g8hhehh55hh97cadb06hkf60h65blh55b56bbda15h'],
};
const DIGITS = '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ';
/** The other new parts: Tomodachi number -> Switch number (null: 'none'). */
// prettier-ignore
const PARTS: Record<string, Record<number, number | null>> = {
  Eye: {60:null,61:48,65:26,68:13,69:18,70:26,71:33,72:25,73:3,74:24,75:15,76:27,77:53,78:41,79:25,80:26},
  Eyebrow: {23:null,24:23,25:7,26:4,27:7,28:19,29:7,30:5,31:4,32:7,33:9,34:19,35:12,36:16,37:6,38:16,39:19,40:13,41:6,42:21},
  Nose: {19:null,20:13,21:1,22:1,23:4,24:2,25:5,26:1,27:1,28:2,29:0,30:0,31:0,32:4},
  Mouth: {36:null,40:23,41:30,42:26,43:28,44:2,45:2,46:24,47:2,48:34,49:6,50:23,51:24,52:19,53:29,54:6,55:3},
  Faceline: {18:0,19:1,20:9,21:5},
  Mustache: {6:1,7:0,8:3,9:2,10:0,11:1,12:1,13:3,14:0,15:null},
  Beard: {6:4,7:3,8:3,9:1,10:2,11:1,12:2,13:3},
  BeardShort: {0:null,1:2,2:2,3:2,4:2,5:2,6:2,7:2,8:2,9:2,10:2,11:2,12:2,13:2,14:2},
};

/** Switch hair from the game's: [type, mirrored]. */
export function switchHair(type: number, front: number): [number, boolean] {
  if (type < 132) return [type, false];
  const back = HAIR_BACK[type];
  const v = back ? (back[0][DIGITS.indexOf(back[1][front] ?? '0')] ?? back[0][0]) : HAIR[type];
  if (v === undefined) return [33, false];
  return [v % 132, v >= 132];
}

/** A part that's the Switch's under the same number, or its closest one. */
function part(kind: string, v: number, count: number, none = 0): number {
  const m = PARTS[kind];
  if (m && v in m) return m[v] ?? none;
  return v < count ? v : none;
}

const rgb = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const near = (hex: string, list: string[]) => {
  const [r, g, b] = rgb(hex);
  let best = 0,
    d = Infinity;
  list.forEach((h, i) => {
    const [R, G, B] = rgb(h);
    const e = (r - R) ** 2 + (g - G) ** 2 + (b - B) ** 2;
    if (e < d) ((d = e), (best = i));
  });
  return best;
};

/** Skin: 100-109 are the Switch's ten, 110-113 the game's new ones (by their
 * place in its light-to-dark order), under 100 a common colour (an alien). */
function skin(c: number): number {
  if (c >= 100 && c < 110) return c - 100;
  if (c >= 110) return ({ 110: 0, 111: 1, 112: 2, 113: 8 } as Record<number, number>)[c] ?? 0;
  return near(COMMON[c] ?? SKIN[0], SKIN);
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** CharInfoEx -> Switch CharInfo (0x58 bytes, what miijs reads). `name` =
 * the Mii's name in the game (CharInfoEx's own is 'no name' for Miis made
 * there); `favorite`: the game keeps no favourite colour. */
export function toCharInfo(ex: Uint8Array, name = '', favorite = 0): Uint8Array {
  const g = (k: keyof typeof EX) => ex[EX[k]];
  const c = new Uint8Array(0x58);
  c.set(ex.subarray(0, 0x10)); // create ID
  // the name: the game's own (a Mii made there has 'no name' in CharInfoEx)
  const nm = name || new TextDecoder('utf-16le').decode(ex.subarray(EX.name, EX.name + 0x16)).replace(/\0.*$/s, '');
  [...nm.slice(0, 10)].forEach((ch, i) => {
    const u = ch.charCodeAt(0);
    c[0x10 + 2 * i] = u & 0xff;
    c[0x11 + 2 * i] = u >> 8;
  });
  const color = (v: number) => (v < 100 ? v : 0);
  const [hair, flip] = switchHair(ex[EX.hairType] | (ex[EX.hairType + 1] << 8), g('hairTypeFront'));
  const eyebrow = part('Eyebrow', g('eyebrowType'), 24, 23);
  const out = [
    g('fontRegion') & 3,
    favorite % 12,
    g('gender') & 1,
    clamp(g('height'), 0, 127),
    clamp(g('build'), 0, 127),
    0,
    g('regionMove') & 3,
    part('Faceline', g('facelineType'), 12),
    skin(g('facelineColor')),
    0,
    0,
    hair,
    color(g('hairColor0')),
    flip ? 1 : 0,
    part('Eye', g('eyeType'), 60),
    color(g('eyeColor')),
    clamp(g('eyeScale'), 0, 7),
    clamp(g('eyeAspect'), 0, 6),
    clamp(g('eyeRotate'), 0, 7),
    clamp(g('eyeX'), 0, 12),
    clamp(g('eyeY'), 0, 18),
    eyebrow,
    color(g('eyebrowColor')),
    g('eyebrowType') === 23 ? 0 : clamp(g('eyebrowScale'), 0, 8),
    clamp(g('eyebrowAspect'), 0, 6),
    clamp(g('eyebrowRotate'), 0, 11),
    clamp(g('eyebrowX') - 2, 0, 12),
    clamp(g('eyebrowY'), 3, 18),
    part('Nose', g('noseType'), 18, 1),
    clamp(g('noseScale'), 0, 8),
    clamp(g('noseY'), 0, 18),
    part('Mouth', g('mouthType'), 36),
    color(g('mouthColor')),
    clamp(g('mouthScale'), 0, 8),
    clamp(g('mouthAspect'), 0, 6),
    clamp(g('mouthY'), 0, 18),
    color(g('beardColor')),
    part('Beard', g('beardType'), 6),
    part('Mustache', g('mustacheType'), 6),
    clamp(g('mustacheScale'), 0, 8),
    clamp(g('mustacheY'), 0, 16),
    g('glassType') < 20 ? g('glassType') : 0,
    color(g('glassColor')),
    clamp(g('glassScale'), 0, 7),
    clamp(g('glassY'), 0, 20),
    g('faceFlags') & MOLE_ON ? 1 : 0,
    clamp(g('moleScale'), 0, 8),
    clamp(g('moleX'), 0, 16),
    clamp(g('moleY'), 0, 30),
  ];
  c.set(out, 0x26);
  return c;
}
