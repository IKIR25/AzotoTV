// What the AzotoTV home needs from an "app": a tile on the bottom row and the
// "top shelf" it fills the screen with while its tile has focus. Adding an app
// (e.g. games) = one module exporting an `App`, listed in `./index.ts`.

/** One page of the top shelf. */
export type Slide = {
  /** unique within the app's shelf */
  key: string;
  title: string;
  /** title art (a film's logo) shown instead of the title */
  art?: string | null;
  /** square picture above the title (album cover, game box art…) */
  cover?: string | null;
  /** small line above the title, with an optional icon (a channel logo) */
  kicker?: { icon?: string | null; text: string };
  /** "● In diretta" next to the kicker */
  live?: boolean;
  meta: string[];
  pill?: string;
  overview?: string;
  /** 0..1 progress bar (programme on air, film part-watched, song playing) */
  progress?: number;
  backdrop: string | null;
  /** show the backdrop blurred (a logo or a cover rather than a picture) */
  ambient?: boolean;
  /** label of the button shown while the shelf has focus */
  action: string;
  open: () => void;
};

export interface App {
  /** stable id: saved in the tile order, used as `data-key="app:<key>"` */
  key: string;
  name: string;
  /** one line for the Store */
  description: string;
  /** always on the home (the Store itself): can't be removed */
  fixed?: boolean;
  /** where A on the tile goes */
  route: string;
  /** tile background (CSS) */
  color: string;
  /** tile glyph: SVG markup in a 48×48 viewBox, white strokes; `class="fill"` fills */
  icon: string;
  /** the shelf; read reactive state here so the home updates by itself */
  slides: () => Slide[];
  /**
   * Fetch what the shelf shows for this profile. Called when the home opens,
   * after a profile switch and after playback; throw if the server isn't
   * reachable and the home retries a few seconds later.
   */
  load?: (user: string) => Promise<void>;
}
