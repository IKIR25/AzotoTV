//! YouTube without an account: search, channels, a video's details and its
//! related videos through YouTube's own web API ("innertube", as a signed-out
//! browser), subscriptions through the channels' public feeds. Nothing here is
//! per profile: subscriptions, history and likes live in the app (JS), keyed by
//! the Jellyfin user like the rest of AzotoTV. Playback is mpv + yt-dlp
//! (player.rs), streamed, never downloaded.
//!
//! YouTube's answers are big nested JSON whose layout changes often, so videos
//! are picked out wherever they appear (by renderer name) instead of following
//! a fixed path.

use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Semaphore;

use crate::jellyfin::cache_dir;

const API: &str = "https://www.youtube.com/youtubei/v1";
/// used until the live one is read from youtube.com (any recent one works)
const FALLBACK_VERSION: &str = "2.20261016.01.00";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
/// search filter "type: video" (protobuf, as YouTube's own filter chips send it)
const ONLY_VIDEOS: &str = "EgIQAQ==";
/// search "most viewed first", filter "uploaded this week, type: video"
const THIS_WEEK: &str = "CAMSBAgDEAE=";
/// YouTube Music's search filter "Brani" (songs only: official tracks with artist and album)
const ONLY_SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA==";
const MUSIC_FALLBACK_VERSION: &str = "1.20261013.15.00";
/// a channel's "Video" tab
const VIDEOS_TAB: &str = "EgZ2aWRlb3PyBgQKAjoA";
/// a channel's feed is fetched at most this often
const FEED_TTL: Duration = Duration::from_secs(15 * 60);
/// thumbnails older than this are dropped from the disk cache at startup
const THUMB_DAYS: u64 = 21;

#[derive(Clone, Serialize, Default)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub channel_id: String,
    /// seconds, 0 = unknown (or live)
    pub length: u32,
    /// as YouTube writes it, in Italian ("1,2 Mln di visualizzazioni")
    pub views: String,
    /// "3 giorni fa" (search, related) or empty when `ts` is set (feeds)
    pub published: String,
    /// unix seconds of publication (feeds only)
    pub ts: i64,
    pub short: bool,
    pub live: bool,
}

#[derive(Clone, Serialize, Default)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// "1,25 Mln di iscritti"
    pub subs: String,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub videos: Vec<Video>,
    pub shorts: Vec<Video>,
    pub channels: Vec<Channel>,
}

#[derive(Serialize)]
pub struct VideoPage {
    pub video: Video,
    pub description: String,
    pub date: String,
    pub owner: Channel,
    pub related: Vec<Video>,
    pub shorts: Vec<Video>,
    /// where the comments start (`comments()`), None = comments are off
    pub comments: Option<String>,
}

#[derive(Serialize)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub avatar: Option<String>,
    pub text: String,
    /// "4 giorni fa"
    pub published: String,
    /// "5,8K", empty = none
    pub likes: String,
    /// "174", empty = none
    pub replies: String,
    /// written by the video's own channel
    pub creator: bool,
}

#[derive(Serialize)]
pub struct Comments {
    /// "2789 commenti" (first page only)
    pub count: String,
    pub comments: Vec<Comment>,
    /// the next page, if any
    pub next: Option<String>,
}

#[derive(Serialize)]
pub struct ChannelPage {
    pub channel: Channel,
    pub banner: Option<String>,
    pub description: String,
    pub videos: Vec<Video>,
}

/// A song from YouTube Music's search (Musica app).
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// seconds
    pub length: u32,
    /// square album art (googleusercontent URL), if any
    pub cover: Option<String>,
}

pub struct Yt {
    client: reqwest::Client,
    version: Mutex<Option<String>>,
    music_version: Mutex<Option<String>>,
    feeds: Mutex<HashMap<String, (Instant, Vec<Video>)>>,
}

impl Yt {
    pub fn new() -> Arc<Self> {
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(15))
            .build()
            .expect("http client");
        Arc::new(Self { client, version: Mutex::new(None), music_version: Mutex::new(None), feeds: Mutex::new(HashMap::new()) })
    }

    /// The web client's current version, read once from youtube.com.
    async fn version(&self) -> String {
        if let Some(v) = self.version.lock().unwrap().clone() {
            return v;
        }
        let v = self.read_version("https://www.youtube.com/").await.unwrap_or_else(|| FALLBACK_VERSION.into());
        *self.version.lock().unwrap() = Some(v.clone());
        v
    }

    /// The same for YouTube Music's web client.
    async fn music_version(&self) -> String {
        if let Some(v) = self.music_version.lock().unwrap().clone() {
            return v;
        }
        let v = self.read_version("https://music.youtube.com/").await.unwrap_or_else(|| MUSIC_FALLBACK_VERSION.into());
        *self.music_version.lock().unwrap() = Some(v.clone());
        v
    }

    async fn read_version(&self, url: &str) -> Option<String> {
        let page = async {
            self.client
                .get(url)
                .header("Cookie", "SOCS=CAI")
                .send()
                .await
                .ok()?
                .text()
                .await
                .ok()
        };
        page.await.and_then(|html| {
            let key = "\"INNERTUBE_CLIENT_VERSION\":\"";
            let at = html.find(key)? + key.len();
            let end = html[at..].find('"')?;
            Some(html[at..at + end].to_string())
        })
    }

    async fn call(&self, endpoint: &str, mut body: Value) -> Result<Value, String> {
        body["context"] = json!({
            "client": { "clientName": "WEB", "clientVersion": self.version().await, "hl": crate::locale::lang(), "gl": crate::locale::region() }
        });
        let resp = self
            .client
            .post(format!("{API}/{endpoint}?prettyPrint=false"))
            .header("Cookie", "SOCS=CAI")
            .json(&body)
            .send()
            .await
            .map_err(|e| t!("Can't reach YouTube: {e}", e = e))?;
        if !resp.status().is_success() {
            return Err(format!("YouTube HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    /// Songs on YouTube Music (official tracks, not videos), best match first.
    pub async fn songs(&self, query: &str) -> Result<Vec<Song>, String> {
        let body = json!({
            "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": self.music_version().await, "hl": crate::locale::lang(), "gl": crate::locale::region() } },
            "query": query,
            "params": ONLY_SONGS,
        });
        let resp = self
            .client
            .post("https://music.youtube.com/youtubei/v1/search?prettyPrint=false")
            .header("Cookie", "SOCS=CAI")
            .header("Origin", "https://music.youtube.com")
            .json(&body)
            .send()
            .await
            .map_err(|e| t!("Can't reach YouTube Music: {e}", e = e))?;
        if !resp.status().is_success() {
            return Err(format!("YouTube Music HTTP {}", resp.status()));
        }
        let v: Value = resp.json().await.map_err(|e| e.to_string())?;
        let mut out: Vec<Song> = find_all(&v, "musicResponsiveListItemRenderer").into_iter().filter_map(song_item).collect();
        let mut seen = std::collections::HashSet::new();
        out.retain(|s| seen.insert(s.id.clone()));
        Ok(out)
    }

    pub async fn search(&self, query: &str, recent: bool) -> Result<SearchResult, String> {
        let params = if recent { THIS_WEEK } else { ONLY_VIDEOS };
        // the "videos" filter keeps Shorts and channels out; a plain search
        // alongside brings the channels (and its Shorts, kept apart)
        let (vids, all) = tokio::join!(
            self.call("search", json!({ "query": query, "params": params })),
            async {
                if recent {
                    Ok(Value::Null)
                } else {
                    self.call("search", json!({ "query": query })).await
                }
            }
        );
        let vids = vids?;
        let all = all.unwrap_or(Value::Null);
        let (videos, mut shorts) = split(videos_in(&vids));
        shorts.extend(videos_in(&all).into_iter().filter(|v| v.short));
        dedup(&mut shorts);
        let channels = find_all(&all, "channelRenderer").into_iter().filter_map(channel_renderer).collect();
        Ok(SearchResult { videos, shorts, channels })
    }

    pub async fn video(&self, id: &str) -> Result<VideoPage, String> {
        let v = self.call("next", json!({ "videoId": id })).await?;
        let results = &v["contents"]["twoColumnWatchNextResults"];
        let primary = find_all(&results["results"], "videoPrimaryInfoRenderer").into_iter().next().cloned();
        let secondary = find_all(&results["results"], "videoSecondaryInfoRenderer").into_iter().next().cloned();
        let (primary, secondary) = match (primary, secondary) {
            (Some(p), Some(s)) => (p, s),
            _ => return Err(t!("video not available")),
        };
        let o = &secondary["owner"]["videoOwnerRenderer"];
        let owner = match o["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str() {
            Some(id) => Channel {
                id: id.into(),
                name: text(&o["title"]),
                avatar: last_thumb(&o["thumbnail"]).map(|u| avatar_url(&u)),
                subs: text(&o["subscriberCountText"]),
            },
            // a collaboration: the owner opens a list of the channels; take the first
            None => {
                let first = find_all(&o["navigationEndpoint"], "listItemViewModel").into_iter().next().cloned().unwrap_or(Value::Null);
                Channel {
                    id: find_all(&first, "browseId").into_iter().find_map(|b| b.as_str()).unwrap_or_default().into(),
                    name: Some(text(&first["title"])).filter(|t| !t.is_empty()).unwrap_or_else(|| text(&o["attributedTitle"])),
                    avatar: largest_source(&first["leadingAccessory"])
                        .or_else(|| largest_source(&o["avatarStack"]))
                        .map(|u| avatar_url(&u)),
                    subs: text(&first["subtitle"]).split('•').find(|p| is_subscribers(p)).unwrap_or_default().trim().into(),
                }
            }
        };
        let views = text(&primary["viewCount"]["videoViewCountRenderer"]["shortViewCount"]);
        let video = Video {
            id: id.into(),
            title: text(&primary["title"]),
            channel: owner.name.clone(),
            channel_id: owner.id.clone(),
            views: if views.is_empty() { text(&primary["viewCount"]["videoViewCountRenderer"]["viewCount"]) } else { views },
            published: text(&primary["relativeDateText"]),
            live: primary["viewCount"]["videoViewCountRenderer"]["isLive"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        let mut related = videos_in(&results["secondaryResults"]);
        // Shorts can sit anywhere on the page (a shelf under the video)
        related.extend(videos_in(&results["results"]).into_iter().filter(|v| v.short));
        related.retain(|r| r.id != id);
        dedup(&mut related);
        let (related, shorts) = split(related);
        let comments = find_all(&results["results"], "itemSectionRenderer")
            .into_iter()
            .find(|s| s["sectionIdentifier"].as_str() == Some("comment-item-section"))
            .and_then(|s| find_all(s, "token").into_iter().find_map(|t| t.as_str()))
            .map(str::to_string);
        Ok(VideoPage {
            video,
            description: secondary["attributedDescription"]["content"].as_str().unwrap_or_default().into(),
            date: text(&primary["dateText"]),
            owner,
            related,
            shorts,
            comments,
        })
    }

    /// A page of top-level comments (most popular first). The comments come as
    /// a list of keys plus, apart, one "entity" per comment with its content.
    pub async fn comments(&self, token: &str) -> Result<Comments, String> {
        let v = self.call("next", json!({ "continuation": token })).await?;
        let mut by_key: HashMap<String, &Value> = HashMap::new();
        for e in find_all(&v["frameworkUpdates"], "commentEntityPayload") {
            if let Some(k) = e["key"].as_str() {
                by_key.insert(k.to_string(), e);
            }
        }
        let comments = find_all(&v["onResponseReceivedEndpoints"], "commentViewModel")
            .into_iter()
            .filter_map(|m| {
                // (a thread holds `commentViewModel: { commentViewModel: {…} }`)
                let m = if m["commentViewModel"].is_object() { &m["commentViewModel"] } else { m };
                let e = by_key.get(m["commentKey"].as_str()?)?;
                let p = &e["properties"];
                let a = &e["author"];
                Some(Comment {
                    id: p["commentId"].as_str().unwrap_or_default().into(),
                    author: a["displayName"].as_str().unwrap_or_default().into(),
                    avatar: a["avatarThumbnailUrl"].as_str().map(str::to_string),
                    text: text(&p["content"]),
                    published: p["publishedTime"].as_str().unwrap_or_default().into(),
                    likes: e["toolbar"]["likeCountNotliked"].as_str().unwrap_or_default().trim().into(),
                    replies: e["toolbar"]["replyCount"].as_str().unwrap_or_default().trim().into(),
                    creator: a["isCreator"].as_bool().unwrap_or(false),
                })
            })
            .collect();
        // the page's last item, when there are more
        let next = find_all(&v["onResponseReceivedEndpoints"], "continuationItemRenderer")
            .into_iter()
            .filter_map(|c| find_all(c, "token").into_iter().find_map(|t| t.as_str()))
            .last()
            .map(str::to_string);
        let count = find_all(&v["onResponseReceivedEndpoints"], "commentsHeaderRenderer")
            .first()
            .map(|h| text(&h["countText"]))
            .unwrap_or_default();
        Ok(Comments { count, comments, next })
    }

    pub async fn channel(&self, id: &str) -> Result<ChannelPage, String> {
        let v = self.call("browse", json!({ "browseId": id, "params": VIDEOS_TAB })).await?;
        let header = find_all(&v["header"], "pageHeaderViewModel").into_iter().next().cloned().unwrap_or(Value::Null);
        let meta = &v["metadata"]["channelMetadataRenderer"];
        let name = meta["title"].as_str().map(str::to_string).unwrap_or_else(|| text(&header["title"]));
        // "@handle" row, then "N iscritti • N video"
        let subs = find_all(&header["metadata"], "metadataParts")
            .into_iter()
            .flat_map(|p| p.as_array().cloned().unwrap_or_default())
            .map(|p| text(&p["text"]))
            .find(|t| is_subscribers(t))
            .unwrap_or_default();
        let channel = Channel {
            id: id.into(),
            name: name.clone(),
            avatar: largest_source(&header["image"]).map(|u| avatar_url(&u)),
            subs,
        };
        let mut videos = videos_in(&v["contents"]);
        for x in &mut videos {
            if x.channel.is_empty() {
                x.channel = name.clone();
                x.channel_id = id.into();
            }
        }
        videos.retain(|x| !x.short);
        Ok(ChannelPage {
            channel,
            banner: largest_source(&header["banner"]),
            description: meta["description"].as_str().unwrap_or_default().into(),
            videos,
        })
    }

    /// Newest uploads of `channels`, newest first: their normal videos, or only
    /// their Shorts. YouTube keeps both as public playlists (UULF… / UUSH…)
    /// with an Atom feed each, no account or key needed.
    pub async fn feed(self: &Arc<Self>, channels: Vec<String>, shorts: bool) -> Vec<Video> {
        let prefix = if shorts { "UUSH" } else { "UULF" };
        let gate = Arc::new(Semaphore::new(6));
        let mut jobs = tokio::task::JoinSet::new();
        for ch in channels {
            let Some(rest) = ch.strip_prefix("UC") else { continue };
            let list = format!("{prefix}{rest}");
            let (me, gate) = (self.clone(), gate.clone());
            jobs.spawn(async move {
                if let Some((t, v)) = me.feeds.lock().unwrap().get(&list) {
                    if t.elapsed() < FEED_TTL {
                        return v.clone();
                    }
                }
                let _permit = gate.acquire().await;
                let url = format!("https://www.youtube.com/feeds/videos.xml?playlist_id={list}");
                let got = match me.client.get(&url).send().await {
                    Ok(r) if r.status().is_success() => r.text().await.ok().map(|x| parse_feed(&x, shorts)),
                    // 404: the channel has no videos of that kind
                    Ok(_) => Some(vec![]),
                    Err(_) => None,
                };
                match got {
                    Some(v) => {
                        me.feeds.lock().unwrap().insert(list, (Instant::now(), v.clone()));
                        v
                    }
                    None => vec![],
                }
            });
        }
        let mut out = vec![];
        while let Some(r) = jobs.join_next().await {
            out.extend(r.unwrap_or_default());
        }
        out.sort_by(|a, b| b.ts.cmp(&a.ts));
        out
    }

    /// `yt://localhost/vi/<id>/<file>` (i.ytimg.com) or `/av/<url-encoded avatar
    /// or banner URL>`, through a disk cache of their own (pruned at startup).
    pub async fn image(&self, path: &str) -> Result<(Vec<u8>, String), String> {
        let urls: Vec<String> = if let Some(rest) = path.strip_prefix("/vi/") {
            let (id, file) = rest.split_once('/').ok_or("bad thumbnail path")?;
            if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err("bad video id".into());
            }
            // not every video has every size: fall back to the always-present one
            let mut files = vec![file.to_string()];
            match file {
                "maxresdefault.jpg" => files.extend(["sddefault.jpg".into(), "hqdefault.jpg".into()]),
                "oardefault.jpg" => files.push("hqdefault.jpg".into()),
                _ => {}
            }
            files.into_iter().map(|f| format!("https://i.ytimg.com/vi/{id}/{f}")).collect()
        } else if let Some(enc) = path.strip_prefix("/av/") {
            let url = percent_decode(enc);
            let host = url.strip_prefix("https://").and_then(|r| r.split('/').next()).unwrap_or_default();
            if !(host == "yt3.ggpht.com" || host.ends_with(".googleusercontent.com")) {
                return Err("bad image host".into());
            }
            vec![url]
        } else {
            return Err("bad image path".into());
        };
        let mut last = String::from("no image");
        for url in urls {
            match cached(&self.client, &url).await {
                Ok(x) => return Ok(x),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

/// Drop old thumbnails: browsing YouTube would otherwise grow the cache forever.
pub fn prune_cache() {
    let Ok(dir) = fs::read_dir(cache_dir().join("yt")) else { return };
    let max = Duration::from_secs(THUMB_DAYS * 86_400);
    for e in dir.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > max);
        if old {
            let _ = fs::remove_file(e.path());
        }
    }
}

async fn cached(client: &reqwest::Client, url: &str) -> Result<(Vec<u8>, String), String> {
    let key = format!("{:016x}", crate::jellyfin::fnv1a(url));
    let dir = cache_dir().join("yt");
    let path = dir.join(&key);
    if let Ok(data) = fs::read(&path) {
        return Ok((data, mime(url).into()));
    }
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("image HTTP {}", resp.status()));
    }
    let data = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    // i.ytimg.com answers a missing maxres with a tiny grey placeholder, not a 404
    if data.len() < 2_000 && url.contains("maxresdefault") {
        return Err("placeholder".into());
    }
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(&path, &data);
    }
    Ok((data, mime(url).into()))
}

fn mime(url: &str) -> &'static str {
    if url.contains("i.ytimg.com") { "image/jpeg" } else { "image/webp" }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A bigger avatar than the one in the answer (`=s88-…` → `=s240-…`).
pub(crate) fn avatar_url(u: &str) -> String {
    let u = if u.starts_with("//") { format!("https:{u}") } else { u.to_string() };
    match u.rfind("=s") {
        Some(i) => format!("{}=s240-c-k-c0x00ffffff-no-rj", &u[..i]),
        None => u,
    }
}

// ---- JSON picking ----

/// Every value under `key`, anywhere in `v` (not looking inside the matches).
pub(crate) fn find_all<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    fn walk<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if k == key {
                        out.push(x);
                    } else {
                        walk(x, key, out);
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, key, out)),
            _ => {}
        }
    }
    let mut out = vec![];
    walk(v, key, &mut out);
    out
}

/// YouTube's text shapes: `simpleText`, `runs`, `content`, or a plain string;
/// without the invisible direction marks it wraps numbers in.
pub(crate) fn text(v: &Value) -> String {
    raw_text(v).replace(['\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}'], "")
}

fn raw_text(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.into();
    }
    if let Some(s) = v["simpleText"].as_str() {
        return s.into();
    }
    if let Some(s) = v["content"].as_str() {
        return s.into();
    }
    if let Some(runs) = v["runs"].as_array() {
        return runs.iter().filter_map(|r| r["text"].as_str()).collect();
    }
    String::new()
}

pub(crate) fn last_thumb(v: &Value) -> Option<String> {
    v["thumbnails"].as_array()?.last()?["url"].as_str().map(str::to_string)
}

fn largest_source(v: &Value) -> Option<String> {
    find_all(v, "sources")
        .into_iter()
        .filter_map(|s| s.as_array())
        .flatten()
        .max_by_key(|s| s["width"].as_u64().unwrap_or(0))
        .and_then(|s| s["url"].as_str())
        .map(|u| if u.starts_with("//") { format!("https:{u}") } else { u.to_string() })
}

/// "1:02:03" → 3723
pub(crate) fn parse_length(s: &str) -> u32 {
    let parts: Vec<u32> = s.trim().split(':').filter_map(|p| p.parse().ok()).collect();
    if parts.len() < 2 || parts.len() != s.trim().split(':').count() {
        return 0;
    }
    parts.iter().fold(0, |acc, p| acc * 60 + p)
}

/// Every video, Short and channel lockup in an answer, in page order.
fn videos_in(v: &Value) -> Vec<Video> {
    let mut out = vec![];
    fn walk(v: &Value, out: &mut Vec<Video>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    let got = match k.as_str() {
                        "videoRenderer" | "compactVideoRenderer" | "gridVideoRenderer" => video_renderer(x),
                        "lockupViewModel" => lockup(x),
                        "shortsLockupViewModel" => shorts_lockup(x),
                        "reelItemRenderer" => reel_item(x),
                        _ => {
                            walk(x, out);
                            continue;
                        }
                    };
                    out.extend(got);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(v, &mut out);
    dedup(&mut out);
    out
}

pub(crate) fn dedup(v: &mut Vec<Video>) {
    let mut seen = std::collections::HashSet::new();
    v.retain(|x| seen.insert(x.id.clone()));
}

fn split(all: Vec<Video>) -> (Vec<Video>, Vec<Video>) {
    all.into_iter().partition(|v| !v.short)
}

fn video_renderer(x: &Value) -> Option<Video> {
    let id = x["videoId"].as_str()?.to_string();
    let owner = if x["ownerText"].is_null() { &x["longBylineText"] } else { &x["ownerText"] };
    let length_text = text(&x["lengthText"]);
    let views = text(&x["shortViewCountText"]);
    Some(Video {
        id,
        title: text(&x["title"]),
        channel: text(owner),
        channel_id: owner["runs"][0]["navigationEndpoint"]["browseEndpoint"]["browseId"]
            .as_str()
            .unwrap_or_default()
            .into(),
        length: parse_length(&length_text),
        views: if views.is_empty() { text(&x["viewCountText"]) } else { views },
        published: text(&x["publishedTimeText"]),
        live: length_text.is_empty()
            && find_all(&x["badges"], "label").iter().any(|l| l.as_str().is_some_and(|s| s.contains("DIRETTA") || s.contains("LIVE"))),
        ..Default::default()
    })
}

fn lockup(x: &Value) -> Option<Video> {
    if x["contentType"].as_str()? != "LOCKUP_CONTENT_TYPE_VIDEO" {
        return None;
    }
    let id = x["contentId"].as_str()?.to_string();
    let meta = &x["metadata"]["lockupMetadataViewModel"];
    let rows: Vec<Vec<String>> = meta["metadata"]["contentMetadataViewModel"]["metadataRows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| r["metadataParts"].as_array().map(|p| p.iter().map(|p| text(&p["text"])).collect()).unwrap_or_default())
                .filter(|r: &Vec<String>| !r.is_empty())
                .collect()
        })
        .unwrap_or_default();
    // [channel], [views, when] — a channel's own page leaves the channel out
    let (channel, stats) = match rows.as_slice() {
        [] => (String::new(), vec![]),
        [one] => (String::new(), one.clone()),
        [ch, st, ..] => (ch.first().cloned().unwrap_or_default(), st.clone()),
    };
    let badges: Vec<String> = find_all(&x["contentImage"], "thumbnailBadgeViewModel").into_iter().map(|b| text(&b["text"])).collect();
    let length = badges.iter().map(|b| parse_length(b)).find(|l| *l > 0).unwrap_or(0);
    let live = badges.iter().any(|b| b.contains("DIRETTA") || b.contains("LIVE"));
    let channel_id = find_all(&meta["image"], "browseId").into_iter().find_map(|b| b.as_str()).unwrap_or_default().to_string();
    Some(Video {
        id,
        title: text(&meta["title"]),
        channel,
        channel_id,
        length,
        views: stats.first().cloned().unwrap_or_default(),
        published: stats.get(1).cloned().unwrap_or_default(),
        live,
        ..Default::default()
    })
}

fn shorts_lockup(x: &Value) -> Option<Video> {
    let id = x["onTap"]["innertubeCommand"]["reelWatchEndpoint"]["videoId"]
        .as_str()
        .map(str::to_string)
        .or_else(|| x["entityId"].as_str()?.strip_prefix("shorts-shelf-item-").map(str::to_string))?;
    Some(Video {
        id,
        title: text(&x["overlayMetadata"]["primaryText"]),
        views: text(&x["overlayMetadata"]["secondaryText"]),
        short: true,
        ..Default::default()
    })
}

fn reel_item(x: &Value) -> Option<Video> {
    Some(Video {
        id: x["videoId"].as_str()?.into(),
        title: text(&x["headline"]),
        views: text(&x["viewCountText"]),
        short: true,
        ..Default::default()
    })
}

/// A row of YouTube Music's song results: title in the first column,
/// "Artist • Album • 3:45" in the second (artist and album are links).
fn song_item(x: &Value) -> Option<Song> {
    let id = x["playlistItemData"]["videoId"].as_str()?.to_string();
    let col = |i: usize| &x["flexColumns"][i]["musicResponsiveListItemFlexColumnRenderer"]["text"];
    let title = raw_text(col(0));
    let runs = col(1)["runs"].as_array()?;
    let page = |r: &Value| {
        r["navigationEndpoint"]["browseEndpoint"]["browseEndpointContextSupportedConfigs"]["browseEndpointContextMusicConfig"]["pageType"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let pick = |kind: &str| {
        runs.iter()
            .filter(|r| page(r) == kind)
            .map(|r| r["text"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ")
    };
    // songs with no linked artist (rare): the first plain text part
    let parts: Vec<String> = raw_text(col(1)).split(" • ").map(str::to_string).collect();
    let artist = Some(pick("MUSIC_PAGE_TYPE_ARTIST")).filter(|a| !a.is_empty()).unwrap_or_else(|| parts.first().cloned().unwrap_or_default());
    let length = parts.last().map(|p| parse_length(p)).unwrap_or(0);
    let cover = x["thumbnail"]["musicThumbnailRenderer"]["thumbnail"]["thumbnails"]
        .as_array()
        .and_then(|t| t.last())
        .and_then(|t| t["url"].as_str())
        // the list's size is tiny: ask Google for a big square instead
        .map(|u| match u.rfind("=w") {
            Some(i) => format!("{}=w544-h544-l90-rj", &u[..i]),
            None => u.to_string(),
        });
    if title.is_empty() {
        return None;
    }
    Some(Song { id, title, artist, album: pick("MUSIC_PAGE_TYPE_ALBUM"), length, cover })
}

fn channel_renderer(x: &Value) -> Option<Channel> {
    // the "subscriber" field holds the @handle nowadays, the "video count" the subscribers
    let a = text(&x["subscriberCountText"]);
    let b = text(&x["videoCountText"]);
    Some(Channel {
        id: x["channelId"].as_str()?.into(),
        name: text(&x["title"]),
        avatar: last_thumb(&x["thumbnail"]).map(|u| avatar_url(&u)),
        subs: if is_subscribers(&b) { b } else { a },
    })
}

// ---- channel feeds (Atom) ----

fn tag<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let at = s.find(open)? + open.len();
    let end = s[at..].find(close)?;
    Some(&s[at..at + end])
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

fn parse_feed(xml: &str, shorts: bool) -> Vec<Video> {
    let channel_id = tag(xml, "<yt:channelId>", "</yt:channelId>").map(|c| format!("UC{}", c.trim_start_matches("UC"))).unwrap_or_default();
    xml.split("<entry>")
        .skip(1)
        .filter_map(|e| {
            let id = tag(e, "<yt:videoId>", "</yt:videoId>")?.to_string();
            let published = tag(e, "<published>", "</published>").unwrap_or_default();
            Some(Video {
                id,
                title: unescape(tag(e, "<title>", "</title>").unwrap_or_default()),
                channel: unescape(tag(e, "<name>", "</name>").unwrap_or_default()),
                channel_id: channel_id.clone(),
                views: tag(e, "<media:statistics views=\"", "\"").map(|n| t!("{n} views", n = group(n))).unwrap_or_default(),
                ts: parse_rfc3339(published),
                short: shorts,
                ..Default::default()
            })
        })
        .collect()
}

/// "1234567" → "1.234.567"
/// "1.2M subscribers", "1,2 Mln di iscritti": YouTube's wording in the
/// languages it is asked in (locale.rs).
fn is_subscribers(s: &str) -> bool {
    s.contains("subscriber") || s.contains("iscritt")
}

fn group(n: &str) -> String {
    let mut out = String::new();
    for (i, c) in n.chars().enumerate() {
        if i > 0 && (n.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// "2026-10-17T15:00:31+00:00" → unix seconds (feeds are always UTC).
fn parse_rfc3339(s: &str) -> i64 {
    let num = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    let (y, mo, d, h, mi, se) = (num(0, 4), num(5, 7), num(8, 10), num(11, 13), num(14, 16), num(17, 19));
    if y == 0 {
        return 0;
    }
    // days from civil (Howard Hinnant)
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + h * 3600 + mi * 60 + se
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths() {
        assert_eq!(parse_length("29:06"), 1746);
        assert_eq!(parse_length("1:02:03"), 3723);
        assert_eq!(parse_length("LIVE"), 0);
        assert_eq!(parse_length("Shorts"), 0);
    }

    #[test]
    fn dates() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00+00:00"), 0);
        assert_eq!(parse_rfc3339("2026-10-17T15:00:31+00:00"), 1_792_249_231);
    }

    #[test]
    fn decode() {
        assert_eq!(percent_decode("https%3A%2F%2Fyt3.ggpht.com%2Fa%3Ds88"), "https://yt3.ggpht.com/a=s88");
        assert_eq!(group("1234567"), "1.234.567");
        assert_eq!(avatar_url("//yt3.ggpht.com/abc=s88-c-k-no"), "https://yt3.ggpht.com/abc=s240-c-k-c0x00ffffff-no-rj");
    }
}

/// Against the real YouTube: `cargo test --lib youtube::live -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::*;

    fn show(label: &str, v: &[Video]) {
        println!("{label}: {}", v.len());
        for x in v.iter().take(4) {
            println!("  {} | {} | {} ({}) | {}s | {} | {} | ts {} short {} live {}", x.id, x.title, x.channel, x.channel_id, x.length, x.views, x.published, x.ts, x.short, x.live);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn everything() {
        let yt = Yt::new();
        for x in yt.songs("vasco rossi albachiara").await.unwrap().iter().take(4) {
            println!("  song {} | {} | {} | {} | {}s | {:?}", x.id, x.title, x.artist, x.album, x.length, x.cover);
        }
        let s = yt.search("minecraft", false).await.unwrap();
        show("search videos", &s.videos);
        show("search shorts", &s.shorts);
        for c in s.channels.iter().take(3) {
            println!("  channel {} {} {} {:?}", c.id, c.name, c.subs, c.avatar);
        }
        let w = yt.search("ricette pasta", true).await.unwrap();
        show("this week", &w.videos);
        let c = yt.video("u5I51vPOSMI").await.unwrap();
        println!("collab owner: {} {} {} {:?}", c.owner.id, c.owner.name, c.owner.subs, c.owner.avatar);
        let k = yt.comments(c.comments.as_deref().expect("comments token")).await.unwrap();
        println!("comments: {} ({}) next {}", k.comments.len(), k.count, k.next.is_some());
        for x in k.comments.iter().take(3) {
            println!("  {} | {} | {} likes {} replies {} creator {} | {:?}", x.author, x.text.chars().take(60).collect::<String>(), x.published, x.likes, x.replies, x.creator, x.avatar);
        }
        if let Some(n) = &k.next {
            let k2 = yt.comments(n).await.unwrap();
            println!("comments page 2: {} first {}", k2.comments.len(), k2.comments.first().map(|c| c.author.as_str()).unwrap_or(""));
        }
        let p = yt.video("aqz-KE-bpKQ").await.unwrap();
        println!("video: {} | {} | {} | {} | owner {} {} {} {:?}", p.video.title, p.video.views, p.video.published, p.date, p.owner.id, p.owner.name, p.owner.subs, p.owner.avatar);
        println!("  desc {}", p.description.chars().take(80).collect::<String>());
        show("related", &p.related);
        show("related shorts", &p.shorts);
        let c = yt.channel("UCX6OQ3DkcsbYNE6H8uQQuVA").await.unwrap();
        println!("channel: {} {} {:?} banner {:?}", c.channel.name, c.channel.subs, c.channel.avatar, c.banner);
        show("channel videos", &c.videos);
        let f = yt.feed(vec!["UCX6OQ3DkcsbYNE6H8uQQuVA".into(), "UCSMOQeBJ2RAnuFungnQOxLg".into()], false).await;
        show("feed", &f);
        let f = yt.feed(vec!["UCX6OQ3DkcsbYNE6H8uQQuVA".into()], true).await;
        show("feed shorts", &f);
        let (img, ct) = yt.image("/vi/aqz-KE-bpKQ/maxresdefault.jpg").await.unwrap();
        println!("thumb {} bytes {ct}", img.len());
        if let Some(a) = &c.channel.avatar {
            let enc: String = a.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
            let (img, ct) = yt.image(&format!("/av/{enc}")).await.unwrap();
            println!("avatar {} bytes {ct}", img.len());
        }
    }
}
