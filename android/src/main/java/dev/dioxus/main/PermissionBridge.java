package dev.dioxus.main;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Build;

public final class PermissionBridge {
    private PermissionBridge() {}

    public static boolean hasRecordAudioPermission(Context context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) {
            return true;
        }
        return context.checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                == PackageManager.PERMISSION_GRANTED;
    }

    public static boolean requestRecordAudioPermission(Context context, int requestCode) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) {
            return true;
        }
        if (!(context instanceof Activity)) {
            return false;
        }

        final Activity activity = (Activity) context;
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                activity.requestPermissions(
                        new String[] { Manifest.permission.RECORD_AUDIO },
                        requestCode
                );
            }
        });
        return true;
    }
}
