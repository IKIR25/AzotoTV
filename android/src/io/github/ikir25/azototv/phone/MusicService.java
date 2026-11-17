package io.github.ikir25.azototv.phone;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.media.MediaMetadata;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.os.Build;
import android.os.IBinder;

/**
 * Keeps the page's music going with the screen off: a foreground service (so
 * Android doesn't freeze the app) with a media notification and the lock
 * screen's controls. The page plays the sound itself (its <audio>); this only
 * shows what plays and hands the buttons back to it (MainActivity.music).
 */
public class MusicService extends Service {
    static final String CHANNEL = "music";
    static final String ACTION = "io.github.ikir25.azototv.phone.MUSIC";
    private static final int ID = 1;

    private MediaSession session;

    /** What plays now (null: the music stopped, the service goes). */
    static void update(Context c, String title, String artist, boolean playing) {
        Intent i = new Intent(c, MusicService.class);
        if (title == null) {
            c.stopService(i);
            return;
        }
        i.putExtra("title", title).putExtra("artist", artist).putExtra("playing", playing);
        if (Build.VERSION.SDK_INT >= 26) c.startForegroundService(i);
        else c.startService(i);
    }

    @Override
    public void onCreate() {
        super.onCreate();
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationChannel ch = new NotificationChannel(CHANNEL, getString(R.string.music), NotificationManager.IMPORTANCE_LOW);
            ch.setShowBadge(false);
            getSystemService(NotificationManager.class).createNotificationChannel(ch);
        }
        session = new MediaSession(this, "AzotoTV");
        session.setCallback(new MediaSession.Callback() {
            @Override
            public void onPlay() {
                press("toggle");
            }

            @Override
            public void onPause() {
                press("toggle");
            }

            @Override
            public void onSkipToNext() {
                press("next");
            }

            @Override
            public void onSkipToPrevious() {
                press("previous");
            }

            @Override
            public void onStop() {
                press("stop");
            }
        });
        session.setActive(true);
    }

    /** A button: the page does it (MainActivity listens). */
    private void press(String what) {
        Intent i = new Intent(ACTION).setPackage(getPackageName()).putExtra("what", what);
        sendBroadcast(i);
    }

    private PendingIntent action(String what, int code) {
        Intent i = new Intent(ACTION).setPackage(getPackageName()).putExtra("what", what);
        return PendingIntent.getBroadcast(this, code, i, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent == null) {
            stopSelf();
            return START_NOT_STICKY;
        }
        String title = intent.getStringExtra("title");
        String artist = intent.getStringExtra("artist");
        boolean playing = intent.getBooleanExtra("playing", false);

        session.setMetadata(new MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, title)
                .putString(MediaMetadata.METADATA_KEY_ARTIST, artist)
                .build());
        session.setPlaybackState(new PlaybackState.Builder()
                .setActions(PlaybackState.ACTION_PLAY | PlaybackState.ACTION_PAUSE | PlaybackState.ACTION_PLAY_PAUSE
                        | PlaybackState.ACTION_SKIP_TO_NEXT | PlaybackState.ACTION_SKIP_TO_PREVIOUS | PlaybackState.ACTION_STOP)
                .setState(playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED, PlaybackState.PLAYBACK_POSITION_UNKNOWN, 1f)
                .build());

        Intent open = new Intent(this, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        Notification.Builder b = Build.VERSION.SDK_INT >= 26 ? new Notification.Builder(this, CHANNEL) : new Notification.Builder(this);
        b.setSmallIcon(android.R.drawable.ic_media_play)
                .setContentTitle(title)
                .setContentText(artist)
                .setContentIntent(PendingIntent.getActivity(this, 0, open, PendingIntent.FLAG_IMMUTABLE))
                .setDeleteIntent(action("stop", 4))
                .setOngoing(playing)
                .setVisibility(Notification.VISIBILITY_PUBLIC)
                .addAction(new Notification.Action.Builder(android.R.drawable.ic_media_previous, getString(R.string.previous), action("previous", 1)).build())
                .addAction(new Notification.Action.Builder(playing ? android.R.drawable.ic_media_pause : android.R.drawable.ic_media_play,
                        getString(playing ? R.string.pause : R.string.play), action("toggle", 2)).build())
                .addAction(new Notification.Action.Builder(android.R.drawable.ic_media_next, getString(R.string.next), action("next", 3)).build())
                .setStyle(new Notification.MediaStyle().setMediaSession(session.getSessionToken()).setShowActionsInCompactView(0, 1, 2));
        Notification n = b.build();
        if (Build.VERSION.SDK_INT >= 29) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
        else startForeground(ID, n);
        // paused: the notification can be swiped away (that stops the music)
        if (!playing && Build.VERSION.SDK_INT >= 24) stopForeground(STOP_FOREGROUND_DETACH);
        return START_NOT_STICKY;
    }

    @Override
    public void onDestroy() {
        // a paused song's notification was detached from the service: it goes too
        getSystemService(NotificationManager.class).cancel(ID);
        session.release();
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
