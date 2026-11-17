-- AzotoTV on-screen UI for mpv: bottom info bar, audio/subtitle menu,
-- skip-intro / next-episode prompts, volume + pause feedback. Live TV
-- (info.live): the bar shows the programme on air, ▲ ▼ change channel.
-- Controller-first: every action arrives as `script-message azoto-input <key>`
-- (see mpv-input.conf). Drawn with mpv's ASS overlay at a fixed 1920x1080
-- virtual resolution, so it scales to any output.

local mp = require 'mp'
local utils = require 'mp.utils'

local W, H = 1920, 1080

-- colours are ASS BBGGRR
local C = {
  white = 'FFFFFF',
  muted = 'D8CFC8',
  dim = 'A09890',
  accent = 'FF8C7C', -- #7c8cff
  panel = '1C1210', -- #10121c
  black = '000000',
}

local overlay = mp.create_osd_overlay('ass-events')
overlay.res_x, overlay.res_y = W, H

-- ---- state ------------------------------------------------------------------

-- from Rust (user-data/azoto/info). Live TV adds live=true, start/stop (epoch
-- seconds of the programme on air) and upnext ("21:00 · Tg1").
local info = { title = '', subtitle = '', next = nil }
local segments = {} -- { {type='Intro', s=12.0, e=70.0}, ... }
local bar_until = 0
local vol_until = 0
local paused = false
local buffering = false
local menu = nil -- { kind='audio'|'sub', items={{id=,label=,active=}}, cur=1, top=1 }
local prompt_dismissed = {} -- keyed by prompt id
-- from Rust (user-data/azoto/alert): a live TV channel to jump to ("Guarda il Tg1"),
-- { id=, label=, sub=, channel=, ends= (unix s) }
local alert = nil
local last_drawn = false
-- the last input came from the TV remote (the app sends `azoto-input <key> remote`):
-- the hints name its keys, and its arrows open the menus (it has its own volume)
local remote_ui = false

local function now() return mp.get_time() end

-- Text in the interface's language: AzotoTV writes this file with `T`, the
-- dictionary from the English text here to that language, at its top (player.rs).
local T = AZOTO_T or {}
local function tr(s) return T[s] or s end
local function trf(s, name, value) return (tr(s):gsub('{' .. name .. '}', function() return tostring(value) end)) end

local LANG = {
  ita = tr('Italian'), it = tr('Italian'), eng = tr('English'), en = tr('English'),
  fre = tr('French'), fra = tr('French'), fr = tr('French'), ger = tr('German'), deu = tr('German'), de = tr('German'),
  spa = tr('Spanish'), es = tr('Spanish'), por = tr('Portuguese'), pt = tr('Portuguese'), jpn = tr('Japanese'), ja = tr('Japanese'),
  kor = tr('Korean'), ko = tr('Korean'), chi = tr('Chinese'), zho = tr('Chinese'), zh = tr('Chinese'), rus = tr('Russian'), ru = tr('Russian'),
  ara = tr('Arabic'), hin = tr('Hindi'), tur = tr('Turkish'), pol = tr('Polish'), dut = tr('Dutch'), nld = tr('Dutch'),
  swe = tr('Swedish'), nor = tr('Norwegian'), dan = tr('Danish'), fin = tr('Finnish'), gre = tr('Greek'), ell = tr('Greek'),
  cze = tr('Czech'), ces = tr('Czech'), bul = tr('Bulgarian'), est = tr('Estonian'), hun = tr('Hungarian'), rum = tr('Romanian'), ron = tr('Romanian'),
  heb = tr('Hebrew'), tha = tr('Thai'), vie = tr('Vietnamese'), ukr = tr('Ukrainian'), und = tr('Unknown'),
}
local CODEC = {
  eac3 = 'E-AC3', ac3 = 'AC3', dts = 'DTS', truehd = 'TrueHD', aac = 'AAC', flac = 'FLAC',
  opus = 'Opus', mp3 = 'MP3', vorbis = 'Vorbis', pcm_s16le = 'PCM', pcm_s24le = 'PCM',
}

-- ---- ASS helpers --------------------------------------------------------------

local function esc(s)
  s = tostring(s or '')
  s = s:gsub('\\', '\\\\'):gsub('{', '\\{'):gsub('}', '\\}'):gsub('\n', '\\N')
  return s
end

local function shape(path, color, alpha)
  return string.format('{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H%s&\\1a&H%02X&\\p1}%s{\\p0}',
    color, alpha or 0, path)
end

-- an: ASS alignment (1 bottom-left, 7 top-left, 9 top-right, 5 centre, ...)
local function text(x, y, s, size, color, an, opts)
  opts = opts or {}
  return string.format('{\\an%d\\pos(%d,%d)\\fs%d\\b%d\\bord%s\\shad0\\1c&H%s&\\3c&H000000&\\3a&H60&\\1a&H%02X&}%s',
    an or 7, x, y, size, opts.bold and 1 or 0, opts.bord or 0, color or C.white, opts.alpha or 0, esc(s))
end

local function rect(x, y, w, h)
  return string.format('m %d %d l %d %d l %d %d l %d %d', x, y, x + w, y, x + w, y + h, x, y + h)
end

local function rrect(x, y, w, h, r)
  r = math.min(r, w / 2, h / 2)
  local p = {}
  local function pt(a, b) p[#p + 1] = string.format('%d %d', a, b) end
  p[#p + 1] = 'm'; pt(x + r, y)
  p[#p + 1] = 'l'; pt(x + w - r, y)
  p[#p + 1] = 'b'; pt(x + w, y); pt(x + w, y); pt(x + w, y + r)
  p[#p + 1] = 'l'; pt(x + w, y + h - r)
  p[#p + 1] = 'b'; pt(x + w, y + h); pt(x + w, y + h); pt(x + w - r, y + h)
  p[#p + 1] = 'l'; pt(x + r, y + h)
  p[#p + 1] = 'b'; pt(x, y + h); pt(x, y + h); pt(x, y + h - r)
  p[#p + 1] = 'l'; pt(x, y + r)
  p[#p + 1] = 'b'; pt(x, y); pt(x, y); pt(x + r, y)
  return table.concat(p, ' ')
end

local function circle(cx, cy, r)
  local k = r * 0.5523
  local p = {}
  local function pt(a, b) p[#p + 1] = string.format('%d %d', a, b) end
  p[#p + 1] = 'm'; pt(cx - r, cy)
  p[#p + 1] = 'b'; pt(cx - r, cy - k); pt(cx - k, cy - r); pt(cx, cy - r)
  p[#p + 1] = 'b'; pt(cx + k, cy - r); pt(cx + r, cy - k); pt(cx + r, cy)
  p[#p + 1] = 'b'; pt(cx + r, cy + k); pt(cx + k, cy + r); pt(cx, cy + r)
  p[#p + 1] = 'b'; pt(cx - k, cy + r); pt(cx - r, cy + k); pt(cx - r, cy)
  return table.concat(p, ' ')
end

local function fmt_time(s)
  s = math.max(0, math.floor((s or 0) + 0.5))
  local h, m, sec = math.floor(s / 3600), math.floor(s / 60) % 60, s % 60
  if h > 0 then return string.format('%d:%02d:%02d', h, m, sec) end
  return string.format('%d:%02d', m, sec)
end

-- ---- track helpers --------------------------------------------------------------

local function channels_label(n)
  if not n then return nil end
  if n == 1 then return 'Mono' elseif n == 2 then return 'Stereo' end
  if n == 6 then return '5.1' elseif n == 8 then return '7.1' end
  return trf('{n} channels', 'n', n)
end

local function track_label(t)
  local lang = t.lang and (LANG[t.lang:lower()] or t.lang:upper()) or nil
  local parts = {}
  parts[#parts + 1] = lang or (t.title and t.title ~= '' and t.title) or trf('Track {n}', 'n', t.id)
  if lang and t.title and t.title ~= '' and t.title:lower() ~= (t.lang or ''):lower() then
    parts[#parts + 1] = t.title
  end
  if t.type == 'audio' then
    local c = CODEC[(t.codec or ''):lower()] or (t.codec and t.codec:upper())
    local ch = channels_label(t['demux-channel-count'])
    if c and ch then parts[#parts + 1] = c .. ' ' .. ch elseif c then parts[#parts + 1] = c end
  elseif t.forced then
    parts[#parts + 1] = tr('Forced')
  end
  return table.concat(parts, ' · ')
end

local function build_menu(kind)
  local tracks = mp.get_property_native('track-list') or {}
  local items = {}
  local want = (kind == 'audio') and 'audio' or 'sub'
  if kind == 'sub' then
    items[#items + 1] = { id = 'no', label = 'Nessuno', active = (mp.get_property('sid') == 'no') }
  end
  for _, t in ipairs(tracks) do
    if t.type == want then
      items[#items + 1] = { id = t.id, label = track_label(t), active = t.selected and true or false }
    end
  end
  if #items == 0 then return nil end
  local cur = 1
  for i, it in ipairs(items) do if it.active then cur = i end end
  return { kind = kind, items = items, cur = cur, top = 1 }
end

local VISIBLE = 8

local function menu_clamp()
  if menu.cur < menu.top then menu.top = menu.cur end
  if menu.cur > menu.top + VISIBLE - 1 then menu.top = menu.cur - VISIBLE + 1 end
end

-- ---- segments / prompts --------------------------------------------------------

local PROMPT_LABEL = { Intro = tr('Skip intro'), Recap = tr('Skip recap'), Outro = tr('Skip credits') }

-- returns { id=, label=, target= (seconds) | nil, to_end=bool, channel= } or nil
local function active_prompt()
  if alert and os.time() >= (alert.ends or 0) then alert = nil end
  if alert and not prompt_dismissed[alert.id] then
    return { id = alert.id, label = alert.label, sub = alert.sub, channel = alert.channel }
  end
  local pos = mp.get_property_number('time-pos')
  local dur = mp.get_property_number('duration')
  if not pos then return nil end
  for i, s in ipairs(segments) do
    local id = 'seg' .. i
    if not prompt_dismissed[id] and pos >= s.s - 0.5 and pos < s.e - 1.0 and (pos - s.s) < 12 then
      if s.type == 'Outro' and info.next then
        return { id = id, label = tr('Next episode'), sub = info.next, to_end = true }
      end
      return { id = id, label = PROMPT_LABEL[s.type] or tr('Skip'), target = s.e }
    end
  end
  if info.next and dur and dur > 60 and (dur - pos) <= 25 and not prompt_dismissed['next'] then
    return { id = 'next', label = tr('Next episode'), sub = info.next, to_end = true }
  end
  return nil
end

-- ---- drawing ---------------------------------------------------------------------

local function badge(lines, x, y, letter)
  lines[#lines + 1] = shape(circle(x, y, 18), C.accent, 0x10)
  lines[#lines + 1] = text(x, y, letter, #letter > 1 and 17 or 24, C.white, 5, { bold = true })
end

local function legend(lines, y)
  local play = paused and tr('Play') or tr('Pause')
  local items, right
  if remote_ui and info.live then
    items = { { 'OK', play }, { '◀', 'Audio' }, { '▶', tr('Subtitles') }, { '↩', tr('Exit') } }
    right = '▲ ▼  ' .. tr('Channel')
  elseif remote_ui then
    items = { { 'OK', play }, { '▼', 'Audio' }, { '▲', tr('Subtitles') }, { '↩', tr('Exit') } }
    right = '◀ ▶  ±10 s'
  else
    items = { { 'A', play }, { 'X', 'Audio' }, { 'Y', tr('Subtitles') }, { 'B', tr('Exit') } }
    right = info.live and ('▲ ▼  ' .. tr('Channel') .. '     ◀ ▶  Volume') or '◀ ▶  ±10 s     ▲ ▼  Volume'
  end
  local x = 96
  for _, it in ipairs(items) do
    badge(lines, x + 17, y, it[1])
    lines[#lines + 1] = text(x + 46, y, it[2], 27, C.muted, 4)
    x = x + 46 + #it[2] * 15 + 44
  end
  lines[#lines + 1] = text(W - 96, y, right, 27, C.dim, 6)
end

local function draw_bar(lines)
  -- gradient scrim
  local top = H - 360
  local n = 30
  for i = 0, n - 1 do
    local y0 = top + math.floor(i * 360 / n)
    local y1 = top + math.floor((i + 1) * 360 / n)
    local a = 255 - math.floor(236 * ((i + 1) / n) ^ 1.7)
    lines[#lines + 1] = shape(rect(0, y0, W, y1 - y0), C.black, a)
  end

  local pos = mp.get_property_number('time-pos') or 0
  local dur = mp.get_property_number('duration') or 0

  lines[#lines + 1] = text(96, H - 232, info.title ~= '' and info.title or mp.get_property('media-title') or '', 60, C.white, 1,
    { bold = true, bord = 1 })
  if info.subtitle ~= '' then
    lines[#lines + 1] = text(96, H - 186, info.subtitle, 34, C.muted, 1, { bord = 1 })
  end

  -- progress track
  local x0, w = 96, W - 192
  local y = H - 148
  if info.live then
    -- the programme on air: its slot, how far into it we are, what's next
    lines[#lines + 1] = shape(rrect(x0, y, w, 10, 5), C.white, 0xA0)
    local start, stop = tonumber(info.start), tonumber(info.stop)
    if start and stop and stop > start then
      local f = math.max(0, math.min(1, (os.time() - start) / (stop - start)))
      if f > 0 then lines[#lines + 1] = shape(rrect(x0, y, math.max(10, math.floor(w * f)), 10, 5), C.accent, 0) end
      lines[#lines + 1] = text(96, H - 118, os.date('%H:%M', start), 32, C.white, 7)
      lines[#lines + 1] = text(W - 96, H - 118, os.date('%H:%M', stop), 32, C.muted, 9)
    end
    local mid = tr('Live')
    if info.upnext and info.upnext ~= '' then mid = mid .. '   ·   ' .. trf('Then {a}', 'a', info.upnext) end
    lines[#lines + 1] = text(math.floor(W / 2), H - 114, mid, 27, C.dim, 8)
    legend(lines, H - 52)
    return
  end
  lines[#lines + 1] = shape(rrect(x0, y, w, 10, 5), C.white, 0xA0)
  local frac = (dur > 0) and math.min(1, pos / dur) or 0
  if frac > 0 then
    lines[#lines + 1] = shape(rrect(x0, y, math.max(10, math.floor(w * frac)), 10, 5), C.accent, 0)
  end
  lines[#lines + 1] = shape(circle(x0 + math.floor(w * frac), y + 5, 15), C.white, 0)

  -- segment markers (intro / outro) as thin ticks on the track
  for _, s in ipairs(segments) do
    if dur > 0 and (s.type == 'Intro' or s.type == 'Outro') then
      lines[#lines + 1] = shape(rect(x0 + math.floor(w * s.s / dur), y - 5, 4, 20), C.accent, 0x30)
    end
  end

  lines[#lines + 1] = text(96, H - 118, fmt_time(pos), 32, C.white, 7)
  if dur > 0 then
    lines[#lines + 1] = text(W - 96, H - 118, fmt_time(dur), 32, C.muted, 9)
    local speed = mp.get_property_number('speed') or 1
    local ends = os.time() + math.floor((dur - pos) / speed)
    lines[#lines + 1] = text(math.floor(W / 2), H - 114, trf('Ends at {time}', 'time', os.date('%H:%M', ends)), 27, C.dim, 8)
  end

  legend(lines, H - 52)
end

local function draw_pause(lines)
  local cx, cy = math.floor(W / 2), math.floor(H / 2) - 40
  lines[#lines + 1] = shape(circle(cx, cy, 92), C.panel, 0x40)
  lines[#lines + 1] = shape(rrect(cx - 34, cy - 40, 22, 80, 7), C.white, 0)
  lines[#lines + 1] = shape(rrect(cx + 12, cy - 40, 22, 80, 7), C.white, 0)
end

local function draw_volume(lines)
  local vol = mp.get_property_number('volume') or 0
  local w, h = 420, 96
  local x, y = W - 96 - w, 72
  lines[#lines + 1] = shape(rrect(x, y, w, h, 28), C.panel, 0x28)
  lines[#lines + 1] = text(x + 34, y + 30, 'Volume', 30, C.white, 4, { bold = true })
  lines[#lines + 1] = text(x + w - 34, y + 30, string.format('%d%%', vol), 30, C.white, 6)
  lines[#lines + 1] = shape(rrect(x + 34, y + 62, w - 68, 12, 6), C.white, 0xA0)
  lines[#lines + 1] = shape(rrect(x + 34, y + 62, math.max(12, math.floor((w - 68) * math.min(vol, 100) / 100)), 12, 6),
    C.accent, 0)
end

local function draw_menu(lines)
  local n = math.min(#menu.items, VISIBLE)
  local pw = 780
  local head, row, foot = 96, 78, 70
  local ph = head + n * row + foot
  local x = W - 96 - pw
  local y = math.floor((H - ph) / 2)

  lines[#lines + 1] = shape(rect(0, 0, W, H), C.black, 0x90) -- dim the video
  lines[#lines + 1] = shape(rrect(x, y, pw, ph, 36), C.panel, 0x14)
  lines[#lines + 1] = text(x + 44, y + 48, menu.kind == 'audio' and 'Audio' or tr('Subtitles'), 42, C.white, 4,
    { bold = true })
  lines[#lines + 1] = text(x + pw - 44, y + 52, string.format('%d / %d', menu.cur, #menu.items), 26, C.dim, 6)

  for i = 0, n - 1 do
    local idx = menu.top + i
    local it = menu.items[idx]
    local ry = y + head + i * row
    local sel = idx == menu.cur
    if sel then
      lines[#lines + 1] = shape(rrect(x + 20, ry, pw - 40, row - 8, 22), C.accent, 0x30)
      lines[#lines + 1] = shape(rrect(x + 20, ry, 6, row - 8, 3), C.accent, 0)
    end
    local mark = it.active and '●  ' or '    '
    lines[#lines + 1] = text(x + 48, ry + math.floor((row - 8) / 2), mark .. it.label, 34, sel and C.white or C.muted, 4,
      { bold = sel })
  end
  if menu.top > 1 then lines[#lines + 1] = text(x + math.floor(pw / 2), y + head - 14, '▲', 22, C.dim, 5) end
  if menu.top + VISIBLE - 1 < #menu.items then
    lines[#lines + 1] = text(x + math.floor(pw / 2), y + head + n * row + 4, '▼', 22, C.dim, 5)
  end
  local foot = remote_ui and tr('OK select    ↩ close    ▲ ▼ scroll') or tr('A select    B close    ▲ ▼ scroll')
  lines[#lines + 1] = text(x + math.floor(pw / 2), y + ph - 34, foot .. tr('    ◀ ▶ audio / subtitles'), 26, C.dim, 5)
end

local function draw_prompt(lines, p, raised)
  local w, h = 560, 124
  local x = W - 96 - w
  local y = raised and (H - 520) or (H - 300)
  lines[#lines + 1] = shape(rrect(x, y, w, h, 34), C.panel, 0x14)
  lines[#lines + 1] = shape(rrect(x, y, 8, h, 4), C.accent, 0)
  badge(lines, x + 60, y + math.floor(h / 2), 'A')
  lines[#lines + 1] = text(x + 108, y + (p.sub and 40 or math.floor(h / 2)), p.label, 40, C.white, 4, { bold = true })
  if p.sub then
    lines[#lines + 1] = text(x + 108, y + 84, p.sub, 28, C.muted, 4)
  end
end

local function draw_loading(lines)
  lines[#lines + 1] = shape(rrect(math.floor(W / 2) - 150, math.floor(H / 2) - 36, 300, 72, 36), C.panel, 0x30)
  lines[#lines + 1] = text(math.floor(W / 2), math.floor(H / 2), tr('Loading…'), 32, C.white, 5)
end

local sub_pos = nil
local function set_sub_pos(v)
  if v ~= sub_pos then
    sub_pos = v
    mp.set_property_number('sub-pos', v)
  end
end

local function render()
  local t = now()
  local bar = paused or t < bar_until or menu ~= nil
  local lines = {}
  set_sub_pos(bar and 72 or 96) -- video subtitles stay clear of the bar

  if menu then
    draw_menu(lines)
  else
    if bar then draw_bar(lines) end
    if paused then draw_pause(lines) end
    if t < vol_until then draw_volume(lines) end
    if buffering and not paused then draw_loading(lines) end
    local p = active_prompt()
    if p then draw_prompt(lines, p, bar) end
  end

  if #lines == 0 then
    if last_drawn then overlay:remove() end
    last_drawn = false
    return
  end
  overlay.data = table.concat(lines, '\n')
  overlay:update()
  last_drawn = true
end

local function show_bar(seconds)
  bar_until = math.max(bar_until, now() + (seconds or 3.5))
  render()
end

-- ---- input -----------------------------------------------------------------------

local function seek(delta)
  mp.commandv('seek', tostring(delta), 'relative')
  show_bar(3.5)
end

local function change_volume(delta)
  mp.commandv('add', 'volume', tostring(delta))
  vol_until = now() + 1.8
  render()
end

local function open_menu(kind)
  local m = build_menu(kind)
  if not m then
    mp.osd_message('', 0)
    return
  end
  menu = m
  menu_clamp()
  render()
end

local function menu_select()
  local it = menu.items[menu.cur]
  if it then
    mp.commandv('set', menu.kind == 'audio' and 'aid' or 'sid', tostring(it.id))
  end
  menu = nil
  show_bar(2.5)
end

local gotos = 0
local function act_prompt(p)
  prompt_dismissed[p.id] = true
  if p.channel then
    -- the app switches channel (or leaves what's playing for it)
    alert = nil
    gotos = gotos + 1
    mp.set_property('user-data/azoto/goto', string.format('%d:%s', gotos, p.channel))
  elseif p.to_end then
    local dur = mp.get_property_number('duration') or 0
    mp.commandv('seek', tostring(math.max(0, dur - 0.2)), 'absolute+exact')
  elseif p.target then
    mp.commandv('seek', tostring(p.target), 'absolute+exact')
  end
  render()
end

-- Live TV: ask the app for the next / previous channel. The app watches this
-- property (the counter makes repeated presses distinct changes).
local zaps = 0
local function zap(dir)
  zaps = zaps + 1
  mp.set_property('user-data/azoto/zap', string.format('%d:%d', zaps, dir))
end

local function on_input(key, src)
  remote_ui = src == 'remote'
  if menu then
    if key == 'up' then menu.cur = (menu.cur - 2) % #menu.items + 1; menu_clamp()
    elseif key == 'down' then menu.cur = menu.cur % #menu.items + 1; menu_clamp()
    elseif key == 'a' then return menu_select()
    elseif key == 'b' or key == 'start' or key == 'stop' then menu = nil; show_bar(1.5); return
    elseif key == 'left' or key == 'right' then
      -- the other menu (stays on this one if the file has no tracks of the other kind)
      local other = build_menu(menu.kind == 'audio' and 'sub' or 'audio')
      if other then menu = other; menu_clamp() end
    elseif key == 'x' then if menu.kind ~= 'audio' then open_menu('audio') else menu = nil end
    elseif key == 'y' then if menu.kind ~= 'sub' then open_menu('sub') else menu = nil end
    end
    return render()
  end

  local p = active_prompt()
  if key == 'a' then
    if p then return act_prompt(p) end
    mp.commandv('cycle', 'pause')
    show_bar(3)
  elseif key == 'b' then
    if p then prompt_dismissed[p.id] = true; return render() end
    mp.commandv('quit')
  elseif key == 'x' then open_menu('audio')
  elseif key == 'y' then open_menu('sub')
  -- the remote's own keys
  elseif key == 'play' then
    mp.commandv('cycle', 'pause')
    show_bar(3)
  elseif key == 'stop' then mp.commandv('quit')
  elseif key == 'chup' or key == 'chdown' then
    if info.live then zap(key == 'chup' and 1 or -1) end
  elseif remote_ui and info.live and (key == 'left' or key == 'right') then open_menu(key == 'left' and 'audio' or 'sub')
  elseif remote_ui and not info.live and (key == 'up' or key == 'down') then open_menu(key == 'down' and 'audio' or 'sub')
  elseif key == 'start' then
    if now() < bar_until then bar_until = 0; render() else show_bar(5) end
  elseif info.live and (key == 'up' or key == 'down') then zap(key == 'up' and 1 or -1)
  elseif info.live and (key == 'left' or key == 'right') then change_volume(key == 'right' and 5 or -5)
  elseif info.live and (key == 'lb' or key == 'rb') then show_bar(3) -- no seeking on live TV
  elseif key == 'left' then seek(-10)
  elseif key == 'right' then seek(10)
  elseif key == 'lb' then seek(-60)
  elseif key == 'rb' then seek(60)
  elseif key == 'up' then change_volume(5)
  elseif key == 'down' then change_volume(-5)
  end
end

mp.register_script_message('azoto-input', on_input)

-- ---- data from the app ----------------------------------------------------------

-- user-data values may arrive as a Lua table, a JSON string, or a JSON string
-- that was itself encoded as a JSON string; accept all three.
local function decode(v)
  for _ = 1, 3 do
    if type(v) == 'string' then v = utils.parse_json(v) else break end
  end
  if type(v) == 'table' then return v end
  return nil
end

local function load_user_data()
  local v = decode(mp.get_property_native('user-data/azoto/info'))
  if v then
    info = { title = v.title or '', subtitle = v.subtitle or '', next = v.next,
             live = v.live and true or false, start = v.start, stop = v.stop, upnext = v.upnext }
    if info.live then show_bar(4) end -- new channel / new programme: show it
  end
  v = decode(mp.get_property_native('user-data/azoto/segments'))
  if v then
    segments = {}
    for _, seg in ipairs(v) do
      if type(seg) == 'table' and seg.s and seg.e then segments[#segments + 1] = seg end
    end
  end
  render()
end

mp.observe_property('user-data/azoto/info', 'string', load_user_data)
mp.observe_property('user-data/azoto/segments', 'string', load_user_data)
mp.observe_property('user-data/azoto/alert', 'string', function(_, v)
  local a = decode(v)
  alert = a and a.channel and a or nil
  render()
end)

mp.observe_property('pause', 'bool', function(_, v)
  paused = v and true or false
  render()
end)
mp.observe_property('paused-for-cache', 'bool', function(_, v)
  buffering = v and true or false
  render()
end)
mp.observe_property('volume', 'number', function() end)

mp.register_event('file-loaded', function()
  prompt_dismissed = {}
  show_bar(5)
end)

-- keep times / prompts fresh while something is on screen (and notice new prompts)
mp.add_periodic_timer(0.5, function()
  if paused or menu or now() < bar_until or now() < vol_until or buffering or last_drawn or #segments > 0 or info.next or alert then
    render()
  end
end)
