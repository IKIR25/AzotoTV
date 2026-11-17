// Sunrise and sunset for the automatic themes. AzotoTV doesn't know where it
// is, so: the longitude of the time zone's own meridian (UTC+1 -> 15° E) and
// latitude 45° N. In Europe that is off by a quarter of an hour or so, which
// is plenty for "light by day, dark by night". (NOAA's approximate formula.)

/** Today's sunrise and sunset, in minutes after local midnight. */
export function sunTimes(d = new Date(), lat = 45): { rise: number; set: number } {
  const year = d.getFullYear();
  // standard time (no daylight saving), in minutes east of UTC
  const std = -Math.max(new Date(year, 0, 1).getTimezoneOffset(), new Date(year, 6, 1).getTimezoneOffset());
  const lon = (std / 60) * 15;
  const rad = Math.PI / 180;
  const day = Math.floor((d.getTime() - new Date(year, 0, 0).getTime()) / 864e5);
  const g = ((2 * Math.PI) / 365) * (day - 1);
  const eqTime =
    229.18 * (0.000075 + 0.001868 * Math.cos(g) - 0.032077 * Math.sin(g) - 0.014615 * Math.cos(2 * g) - 0.040849 * Math.sin(2 * g));
  const decl =
    0.006918 - 0.399912 * Math.cos(g) + 0.070257 * Math.sin(g) - 0.006758 * Math.cos(2 * g) + 0.000907 * Math.sin(2 * g) -
    0.002697 * Math.cos(3 * g) + 0.00148 * Math.sin(3 * g);
  const cosHa = Math.cos(90.833 * rad) / (Math.cos(lat * rad) * Math.cos(decl)) - Math.tan(lat * rad) * Math.tan(decl);
  const ha = Math.acos(Math.min(1, Math.max(-1, cosHa))) / rad;
  const noon = 720 - 4 * lon - eqTime - d.getTimezoneOffset();
  return { rise: noon - 4 * ha, set: noon + 4 * ha };
}

/** Is the sun up now? */
export function isDay(d = new Date()): boolean {
  const { rise, set } = sunTimes(d);
  const now = d.getHours() * 60 + d.getMinutes();
  return now >= rise && now < set;
}
