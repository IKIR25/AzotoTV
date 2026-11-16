package io.github.ikir25.azototv.phone;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.ActivityInfo;
import android.graphics.Insets;
import android.graphics.drawable.ColorDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.provider.MediaStore;
import android.provider.Settings;
import android.view.KeyEvent;
import android.view.WindowManager;
import android.view.WindowInsets;
import android.webkit.JavascriptInterface;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.ArrayList;

/**
 * AzotoTV on an Android phone: finds AzotoTV on the home network and shows the
 * page it serves there (src-tauri/phone, plain http: fine inside an app, unlike
 * a home-screen web app). On top of the page: links and photos shared from
 * other apps ("Manda alla TV", into a Galleria album), the volume keys set the
 * TV's volume while the remote is on screen, vibration on presses.
 */
public class MainActivity extends Activity {
    private static final int BG = 0xff07080f;
    private static final int PICK = 1;

    private final Handler ui = new Handler(Looper.getMainLooper());
    private WebView web;
    private SharedPreferences prefs;
    /** the page's address once AzotoTV answered */
    private String server;
    /** what the page should do once loaded: "#send=<link>" or "#share" */
    private String pending = "";
    /** photos / videos shared from another app, handed to the page's file picker */
    private final ArrayList<Uri> shared = new ArrayList<>();
    private ValueCallback<Uri[]> chooser;
    private volatile boolean volumeKeys;
    private int attempt;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        prefs = getSharedPreferences("azototv", MODE_PRIVATE);
        getWindow().setBackgroundDrawable(new ColorDrawable(BG));
        getWindow().setStatusBarColor(BG);
        getWindow().setNavigationBarColor(BG);

        web = new WebView(this);
        web.setBackgroundColor(BG);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        web.addJavascriptInterface(new Bridge(), "AzotoApp");
        web.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest req) {
                Uri u = req.getUrl();
                if (server != null && u.toString().startsWith(server)) return false;
                if ("about".equals(u.getScheme()) || "data".equals(u.getScheme())) return false;
                // anything else opens in the browser
                try {
                    startActivity(new Intent(Intent.ACTION_VIEW, u));
                } catch (ActivityNotFoundException ignored) {
                }
                return true;
            }

            @Override
            public void onReceivedError(WebView view, WebResourceRequest req, WebResourceError err) {
                if (req.isForMainFrame() && server != null) offline();
            }
        });
        web.setWebChromeClient(new WebChromeClient() {
            @Override
            public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> cb, FileChooserParams params) {
                return pick(cb, params);
            }
        });
        // edge to edge (Android 15+): keep the page clear of the bars and the keyboard
        web.setOnApplyWindowInsetsListener((v, insets) -> {
            if (Build.VERSION.SDK_INT >= 30) {
                Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.ime());
                v.setPadding(bars.left, bars.top, bars.right, bars.bottom);
                return WindowInsets.CONSUMED;
            }
            return insets;
        });
        setContentView(web);
        pending = request(getIntent());
        find();
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        String what = request(intent);
        if (what.isEmpty()) return;
        if (server != null) web.loadUrl(server + "/" + what);
        else pending = what;
    }

    /** What a share asks for (and the shared files, kept for the picker). */
    private String request(Intent i) {
        if (i == null) return "";
        String action = i.getAction();
        String type = i.getType() == null ? "" : i.getType();
        if (Intent.ACTION_SEND.equals(action) && type.startsWith("text/")) {
            CharSequence text = i.getCharSequenceExtra(Intent.EXTRA_TEXT);
            return text == null ? "" : "#send=" + Uri.encode(text.toString());
        }
        if (Intent.ACTION_SEND.equals(action)) {
            Uri u = i.getParcelableExtra(Intent.EXTRA_STREAM);
            if (u == null) return "";
            shared.clear();
            shared.add(u);
            return "#share";
        }
        if (Intent.ACTION_SEND_MULTIPLE.equals(action)) {
            ArrayList<Uri> list = i.getParcelableArrayListExtra(Intent.EXTRA_STREAM);
            if (list == null || list.isEmpty()) return "";
            shared.clear();
            shared.addAll(list);
            return "#share";
        }
        return "";
    }

    // ---- finding AzotoTV ----

    private void find() {
        final int mine = ++attempt;
        web.loadDataWithBaseURL(null, page("Cerco la TV…", "", false), "text/html", "utf-8", null);
        // the address typed the first time (START › Collega un telefono shows it)
        final String saved = prefs.getString("server", null);
        if (saved == null) {
            offline();
            return;
        }
        new Thread(() -> {
            final boolean ok = answers(saved);
            ui.post(() -> {
                if (mine != attempt) return;
                if (ok) open(saved);
                else offline();
            });
        }).start();
    }

    private static boolean answers(String base) {
        HttpURLConnection c = null;
        try {
            c = (HttpURLConnection) new URL(base + "/api/hello").openConnection();
            c.setConnectTimeout(1500);
            c.setReadTimeout(1500);
            if (c.getResponseCode() != 200) return false;
            byte[] buf = new byte[256];
            InputStream in = c.getInputStream();
            int n = in.read(buf);
            return n > 0 && new String(buf, 0, n, "UTF-8").contains("azototv");
        } catch (Exception e) {
            return false;
        } finally {
            if (c != null) c.disconnect();
        }
    }

    private void open(String base) {
        server = base;
        prefs.edit().putString("server", base).apply();
        web.loadUrl(base + "/" + pending);
        pending = "";
    }

    private void offline() {
        server = null;
        String addr = prefs.getString("server", null);
        String html = addr == null
                ? page("Collega la TV", "Il telefono dev'essere sulla Wi-Fi di casa, e AzotoTV acceso.", true)
                : page("La TV non risponde", "Il telefono dev'essere sulla Wi-Fi di casa, e AzotoTV acceso.", true);
        web.loadDataWithBaseURL(null, html.replace("%ADDR%", addr == null ? "" : addr), "text/html", "utf-8", null);
    }

    /** The app's own little page (looking for the TV, or not found). */
    private static String page(String title, String text, boolean form) {
        return "<!doctype html><html><head><meta name=viewport content='width=device-width,initial-scale=1'>"
                + "<style>body{margin:0;min-height:100vh;display:flex;flex-direction:column;justify-content:center;gap:16px;"
                + "padding:24px;box-sizing:border-box;background:#07080f;color:#f4f5fa;font:16px system-ui,sans-serif}"
                + "h1{font-size:24px;margin:0}p{margin:0;color:rgba(244,245,250,.6)}"
                + "input{font:inherit;color:inherit;background:rgba(0,0,0,.35);border:1px solid rgba(255,255,255,.12);border-radius:16px;padding:14px}"
                + "button{font:inherit;font-weight:600;border:0;border-radius:24px;min-height:48px;background:#f4f5fa;color:#10121c}"
                + ".logo{letter-spacing:.24em;font-weight:300}.logo span{color:#e8202e;font-weight:600}.logo b{color:#7c8cff}</style></head><body>"
                + "<div class=logo><span>AZOTO</span><b>TV</b></div><h1>" + title + "</h1><p>" + text + "</p>"
                + (form ? "<p>Indirizzo della TV (START › Collega un telefono):</p>"
                + "<input id=a value='%ADDR%' placeholder='192.168.1.10' inputmode=url autocapitalize=off>"
                + "<button onclick='AzotoApp.retry(document.getElementById(\"a\").value)'>Riprova</button>" : "")
                + "</body></html>";
    }

    // ---- the page's file picker: shared files first, else the photo picker ----

    private boolean pick(ValueCallback<Uri[]> cb, WebChromeClient.FileChooserParams params) {
        if (chooser != null) chooser.onReceiveValue(null);
        chooser = null;
        if (!shared.isEmpty()) {
            cb.onReceiveValue(shared.toArray(new Uri[0]));
            shared.clear();
            return true;
        }
        Intent it;
        if (Build.VERSION.SDK_INT >= 33) {
            it = new Intent(MediaStore.ACTION_PICK_IMAGES);
            if (params.getMode() == WebChromeClient.FileChooserParams.MODE_OPEN_MULTIPLE) {
                it.putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, MediaStore.getPickImagesMaxLimit());
            }
        } else {
            it = params.createIntent();
        }
        try {
            chooser = cb;
            startActivityForResult(it, PICK);
            return true;
        } catch (ActivityNotFoundException e) {
            chooser = null;
            return false;
        }
    }

    @Override
    protected void onActivityResult(int req, int res, Intent data) {
        if (req != PICK || chooser == null) {
            super.onActivityResult(req, res, data);
            return;
        }
        Uri[] out = null;
        if (res == RESULT_OK && data != null) {
            ClipData clip = data.getClipData();
            if (clip != null) {
                out = new Uri[clip.getItemCount()];
                for (int i = 0; i < out.length; i++) out[i] = clip.getItemAt(i).getUri();
            } else if (data.getData() != null) {
                out = new Uri[] {data.getData()};
            }
        }
        chooser.onReceiveValue(out);
        chooser = null;
    }

    // ---- keys ----

    @Override
    public boolean dispatchKeyEvent(KeyEvent e) {
        int code = e.getKeyCode();
        if (volumeKeys && server != null && (code == KeyEvent.KEYCODE_VOLUME_UP || code == KeyEvent.KEYCODE_VOLUME_DOWN)) {
            if (e.getAction() == KeyEvent.ACTION_DOWN) {
                int d = code == KeyEvent.KEYCODE_VOLUME_UP ? 1 : -1;
                web.evaluateJavascript("window.azotoVolume&&window.azotoVolume(" + d + ")", null);
            }
            return true;
        }
        return super.dispatchKeyEvent(e);
    }

    /** Back leaves the app without closing it (the page keeps its place). */
    @Override
    public void onBackPressed() {
        moveTaskToBack(true);
    }

    // ---- what the page can call (window.AzotoApp) ----

    private class Bridge {
        @JavascriptInterface
        public void vibrate(int ms) {
            Vibrator v = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
            if (v == null || !v.hasVibrator()) return;
            if (Build.VERSION.SDK_INT >= 26) v.vibrate(VibrationEffect.createOneShot(Math.max(1, ms), VibrationEffect.DEFAULT_AMPLITUDE));
            else v.vibrate(ms);
        }

        /** The phone's own name ("Galaxy S23 di Luca"), for the TV's list of phones. */
        @JavascriptInterface
        public String deviceName() {
            String name = null;
            if (Build.VERSION.SDK_INT >= 25) name = Settings.Global.getString(getContentResolver(), Settings.Global.DEVICE_NAME);
            if (name == null || name.trim().isEmpty()) {
                String maker = Build.MANUFACTURER == null ? "" : Build.MANUFACTURER;
                String model = Build.MODEL == null ? "" : Build.MODEL;
                name = model.toLowerCase().startsWith(maker.toLowerCase()) ? model : (maker + " " + model);
                if (!name.isEmpty()) name = Character.toUpperCase(name.charAt(0)) + name.substring(1);
            }
            return name.trim();
        }

        @JavascriptInterface
        public int sharedCount() {
            return shared.size();
        }

        @JavascriptInterface
        public void setVolumeKeys(boolean on) {
            volumeKeys = on;
        }

        /** The phone as a game controller: landscape, and the screen stays on. */
        @JavascriptInterface
        public void setLandscape(boolean on) {
            ui.post(() -> {
                setRequestedOrientation(on ? ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE : ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED);
                if (on) getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
                else getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            });
        }

        /** A shared link was sent: back to the app it came from. */
        @JavascriptInterface
        public void sharedDone() {
            ui.post(() -> moveTaskToBack(true));
        }

        /** The address typed on the "not found" page. */
        @JavascriptInterface
        public void retry(String address) {
            String a = address == null ? "" : address.trim().replaceAll("/+$", "");
            if (a.isEmpty()) return;
            if (!a.startsWith("http://") && !a.startsWith("https://")) a = "http://" + a;
            if (!a.matches("https?://[^/]+:\\d+.*")) a = a + ":8686";
            final String base = a;
            ui.post(() -> {
                prefs.edit().putString("server", base).apply();
                find();
            });
        }
    }
}
