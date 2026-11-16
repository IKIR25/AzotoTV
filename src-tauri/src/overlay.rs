//! The notification toast: a small GTK window on the Wayland *overlay* layer
//! (wlr-layer-shell, which KWin supports), so it shows above everything on the
//! screen (the UI, a film or a channel in mpv, a fullscreen game) without ever
//! taking the focus or any input.
//!
//! libgtk-layer-shell is loaded at runtime, not linked: without it (or on X11)
//! `supported()` is false and the UI shows the notification itself instead.
//! Plain GTK widgets, not a second webview: no extra web process (RAM).

use std::{
    cell::RefCell,
    collections::VecDeque,
    ffi::{c_char, c_int, c_void},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gtk::{gdk_pixbuf::PixbufLoader, glib, prelude::*};
use tauri::AppHandle;

/// How long a toast stays up.
const SHOW: Duration = Duration::from_secs(7);
/// Gap before the next queued one.
const GAP: Duration = Duration::from_millis(450);

pub struct Toast {
    /// small coloured line on top ("Pronto da guardare")
    pub kicker: String,
    pub title: String,
    pub body: String,
    /// a poster (tall) or, with `logo`, a channel logo (fitted into a light box)
    pub image: Option<Vec<u8>>,
    pub logo: bool,
}

// ---- libgtk-layer-shell, loaded at runtime -----------------------------------

type Win = *mut c_void;

struct Lib {
    init_for_window: unsafe extern "C" fn(Win),
    set_namespace: unsafe extern "C" fn(Win, *const c_char),
    set_layer: unsafe extern "C" fn(Win, c_int),
    set_anchor: unsafe extern "C" fn(Win, c_int, c_int),
    set_margin: unsafe extern "C" fn(Win, c_int, c_int),
    set_keyboard_mode: unsafe extern "C" fn(Win, c_int),
}

// GtkLayerShellLayer / GtkLayerShellEdge / GtkLayerShellKeyboardMode
const LAYER_OVERLAY: c_int = 3;
const EDGE_RIGHT: c_int = 1;
const EDGE_TOP: c_int = 2;
const KEYBOARD_NONE: c_int = 0;

static LIB: OnceLock<Option<Lib>> = OnceLock::new();
static SUPPORTED: AtomicBool = AtomicBool::new(false);

fn load() -> Option<Lib> {
    unsafe {
        let h = libc::dlopen(c"libgtk-layer-shell.so.0".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if h.is_null() {
            return None;
        }
        let sym = |name: &std::ffi::CStr| {
            let p = libc::dlsym(h, name.as_ptr());
            (!p.is_null()).then_some(p)
        };
        let supported: unsafe extern "C" fn() -> c_int = std::mem::transmute(sym(c"gtk_layer_is_supported")?);
        if supported() == 0 {
            return None; // not Wayland, or no layer-shell in the compositor
        }
        Some(Lib {
            init_for_window: std::mem::transmute(sym(c"gtk_layer_init_for_window")?),
            set_namespace: std::mem::transmute(sym(c"gtk_layer_set_namespace")?),
            set_layer: std::mem::transmute(sym(c"gtk_layer_set_layer")?),
            set_anchor: std::mem::transmute(sym(c"gtk_layer_set_anchor")?),
            set_margin: std::mem::transmute(sym(c"gtk_layer_set_margin")?),
            set_keyboard_mode: std::mem::transmute(sym(c"gtk_layer_set_keyboard_mode")?),
        })
    }
}

/// Call once on the main thread after GTK is up (tauri's `setup`).
pub fn init() {
    let ok = LIB.get_or_init(load).is_some();
    SUPPORTED.store(ok, Ordering::Relaxed);
    eprintln!("[notify] overlay {}", if ok { "on the layer-shell overlay" } else { "unavailable: in-app toasts" });
}

/// The toast can be drawn above other windows.
pub fn supported() -> bool {
    SUPPORTED.load(Ordering::Relaxed)
}

// ---- the window (main thread only) -------------------------------------------

struct Ui {
    win: gtk::Window,
    card: gtk::Box,
    pic: gtk::Box,
    image: gtk::Image,
    kicker: gtk::Label,
    title: gtk::Label,
    body: gtk::Label,
    /// 1.0 at 1920 px wide
    scale: f64,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static QUEUE: RefCell<VecDeque<Toast>> = const { RefCell::new(VecDeque::new()) };
    static BUSY: RefCell<bool> = const { RefCell::new(false) };
}

fn css(s: f64) -> String {
    let px = |v: f64| format!("{}px", (v * s).round());
    format!(
        "window.azoto-toast {{ background: transparent; }}
        .azoto-toast .card {{
            background-color: rgba(16, 18, 32, 0.95); border: 1px solid rgba(255, 255, 255, 0.16);
            border-radius: {r}; padding: {p}; margin: {m};
            box-shadow: 0 {sy} {sb} rgba(0, 0, 0, 0.45);
            opacity: 0; transition: opacity 220ms ease-out;
        }}
        .azoto-toast .card.on {{ opacity: 1; }}
        .azoto-toast .pic {{ border-radius: {pr}; }}
        .azoto-toast .pic.logo {{ background-color: rgba(255, 255, 255, 0.1); }}
        .azoto-toast label {{ font-family: Inter, 'Noto Sans', sans-serif; color: #f4f5fa; }}
        .azoto-toast .kicker {{ font-size: {kf}; font-weight: 700; color: #9aa6ff; letter-spacing: 1px; }}
        .azoto-toast .title {{ font-size: {tf}; font-weight: 700; }}
        .azoto-toast .body {{ font-size: {bf}; color: rgba(244, 245, 250, 0.68); }}",
        r = px(26.0),
        p = px(16.0),
        m = px(24.0),
        sy = px(14.0),
        sb = px(36.0),
        pr = px(12.0),
        kf = px(15.0),
        tf = px(25.0),
        bf = px(17.0),
    )
}

fn build() -> Option<Ui> {
    let lib = LIB.get()?.as_ref()?;
    let scale = gtk::gdk::Display::default()
        .and_then(|d| d.primary_monitor().or_else(|| d.monitor(0)))
        .map(|m| m.geometry().width() as f64 / 1920.0)
        .filter(|s| *s > 0.2)
        .unwrap_or(1.0);
    let px = |v: f64| (v * scale).round() as i32;

    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_decorated(false);
    win.set_resizable(false);
    win.set_accept_focus(false);
    win.set_focus_on_map(false);
    win.set_app_paintable(true);
    if let Some(v) = WidgetExt::screen(&win).and_then(|s| s.rgba_visual()) {
        win.set_visual(Some(&v));
    }
    win.style_context().add_class("azoto-toast");
    unsafe {
        use glib::translate::ToGlibPtr;
        let w: *mut gtk::ffi::GtkWindow = win.to_glib_none().0;
        let w = w as Win;
        (lib.init_for_window)(w);
        (lib.set_namespace)(w, c"azototv-notify".as_ptr());
        (lib.set_layer)(w, LAYER_OVERLAY);
        (lib.set_anchor)(w, EDGE_TOP, 1);
        (lib.set_anchor)(w, EDGE_RIGHT, 1);
        // the card's own CSS margin leaves room for its shadow
        (lib.set_margin)(w, EDGE_TOP, px(24.0));
        (lib.set_margin)(w, EDGE_RIGHT, px(72.0));
        (lib.set_keyboard_mode)(w, KEYBOARD_NONE);
    }
    // clicks / touches go through to whatever is below
    win.input_shape_combine_region(Some(&gtk::cairo::Region::create()));

    let provider = gtk::CssProvider::new();
    if provider.load_from_data(css(scale).as_bytes()).is_ok() {
        gtk::StyleContext::add_provider_for_screen(
            &WidgetExt::screen(&win)?,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let card = gtk::Box::new(gtk::Orientation::Horizontal, px(18.0));
    card.style_context().add_class("card");
    card.set_size_request(px(600.0), -1);

    let pic = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pic.style_context().add_class("pic");
    pic.set_valign(gtk::Align::Center);
    let image = gtk::Image::new();
    image.set_vexpand(true);
    pic.pack_start(&image, true, true, 0);
    card.pack_start(&pic, false, false, 0);

    let text = gtk::Box::new(gtk::Orientation::Vertical, px(3.0));
    text.set_valign(gtk::Align::Center);
    let label = |class: &str, chars: i32, lines: i32| {
        let l = gtk::Label::new(None);
        l.style_context().add_class(class);
        l.set_xalign(0.0);
        l.set_max_width_chars(chars);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        if lines > 1 {
            l.set_line_wrap(true);
            l.set_lines(lines);
        }
        text.pack_start(&l, false, false, 0);
        l
    };
    let kicker = label("kicker", 40, 1);
    let title = label("title", 26, 2);
    let body = label("body", 38, 2);
    card.pack_start(&text, true, true, 0);
    win.add(&card);
    card.show_all();

    Some(Ui { win, card, pic, image, kicker, title, body, scale })
}

fn picture(ui: &Ui, t: &Toast) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let loader = PixbufLoader::new();
    loader.write(t.image.as_ref()?).ok()?;
    loader.close().ok()?;
    let pb = loader.pixbuf()?;
    let (w, h) = (pb.width() as f64, pb.height() as f64);
    // posters 2:3 at 132 px tall; logos fitted into 132 x 74
    let (bw, bh) = if t.logo { (132.0, 74.0) } else { (132.0 * 2.0 / 3.0, 132.0) };
    let k = (bw / w).min(bh / h) * ui.scale;
    pb.scale_simple(
        (w * k).round().max(1.0) as i32,
        (h * k).round().max(1.0) as i32,
        gtk::gdk_pixbuf::InterpType::Bilinear,
    )
}

/// A poster with rounded corners (GTK 3 doesn't clip children to a border-radius).
fn rounded(pb: &gtk::gdk_pixbuf::Pixbuf, r: f64) -> Option<gtk::cairo::ImageSurface> {
    use gtk::gdk::prelude::GdkContextExt;
    use std::f64::consts::{FRAC_PI_2, PI};
    let surf = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, pb.width(), pb.height()).ok()?;
    let cr = gtk::cairo::Context::new(&surf).ok()?;
    let (w, h) = (pb.width() as f64, pb.height() as f64);
    cr.new_sub_path();
    cr.arc(w - r, r, r, -FRAC_PI_2, 0.0);
    cr.arc(w - r, h - r, r, 0.0, FRAC_PI_2);
    cr.arc(r, h - r, r, FRAC_PI_2, PI);
    cr.arc(r, r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
    cr.clip();
    cr.set_source_pixbuf(pb, 0.0, 0.0);
    cr.paint().ok()?;
    drop(cr);
    Some(surf)
}

fn show_next() {
    let Some(t) = QUEUE.with(|q| q.borrow_mut().pop_front()) else {
        BUSY.with(|b| *b.borrow_mut() = false);
        return;
    };
    BUSY.with(|b| *b.borrow_mut() = true);
    UI.with(|u| {
        let mut u = u.borrow_mut();
        if u.is_none() {
            *u = build();
        }
        let Some(ui) = u.as_ref() else { return };
        ui.kicker.set_text(&t.kicker.to_uppercase()); // GTK 3 CSS has no text-transform
        ui.title.set_text(&t.title);
        ui.body.set_text(&t.body);
        ui.body.set_visible(!t.body.is_empty());
        match picture(ui, &t) {
            Some(pb) => {
                match rounded(&pb, 10.0 * ui.scale).filter(|_| !t.logo) {
                    Some(surf) => ui.image.set_from_surface(Some(&surf)),
                    None => ui.image.set_from_pixbuf(Some(&pb)),
                }
                // a poster's box is 88 x 132; other pictures (an achievement's square icon) get their own shape
                let (w, h) = if t.logo { (132.0 * ui.scale, 74.0 * ui.scale) } else { (pb.width() as f64, pb.height() as f64) };
                ui.pic.set_size_request(w as i32, h as i32);
                let sc = ui.pic.style_context();
                if t.logo { sc.add_class("logo") } else { sc.remove_class("logo") }
                ui.pic.show_all();
            }
            None => ui.pic.hide(),
        }
        ui.card.style_context().remove_class("on");
        ui.win.show();
        // next frame: fade in (the CSS transition, finite)
        let card = ui.card.clone();
        glib::timeout_add_local_once(Duration::from_millis(30), move || card.style_context().add_class("on"));
        let card = ui.card.clone();
        let win = ui.win.clone();
        glib::timeout_add_local_once(SHOW, move || {
            card.style_context().remove_class("on");
            glib::timeout_add_local_once(Duration::from_millis(250), move || {
                win.hide();
                glib::timeout_add_local_once(GAP, show_next);
            });
        });
    });
    if UI.with(|u| u.borrow().is_none()) {
        BUSY.with(|b| *b.borrow_mut() = false);
    }
}

/// Queue a toast (any thread). Returns false when the overlay isn't available.
pub fn show(app: &AppHandle, t: Toast) -> bool {
    if !supported() {
        return false;
    }
    let _ = app.run_on_main_thread(move || {
        QUEUE.with(|q| q.borrow_mut().push_back(t));
        if !BUSY.with(|b| *b.borrow()) {
            show_next();
        }
    });
    true
}
