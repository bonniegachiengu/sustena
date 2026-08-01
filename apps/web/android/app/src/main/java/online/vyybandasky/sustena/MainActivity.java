package online.vyybandasky.sustena;

import android.content.Intent;
import android.os.Bundle;

import com.getcapacitor.BridgeActivity;

public class MainActivity extends BridgeActivity {
    @Override
    public void onCreate(Bundle savedInstanceState) {
        // Must be registered before super.onCreate() -- Capacitor's bridge
        // initializes (and starts dispatching JS plugin calls) during the
        // super call, so a plugin registered after it would miss any call
        // made before registration completed.
        registerPlugin(SmsCapturePlugin.class);
        super.onCreate(savedInstanceState);
        capturePendingClassifyIntent(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        capturePendingClassifyIntent(intent);
    }

    /**
     * A tap on the native "Orchie needs a decision" notification
     * (IngestWorker) carries its target sustain/message as intent extras --
     * stash them in SmsAuthStore so JS can pick them up via
     * SmsCapture.consumePendingClassifyTarget() the moment it's ready,
     * regardless of whether this was a cold launch (onCreate) or the app
     * was already running (android:launchMode="singleTask" routes a repeat
     * tap through onNewIntent instead of a fresh onCreate).
     */
    private void capturePendingClassifyIntent(Intent intent) {
        if (intent == null) return;
        String sustainId = intent.getStringExtra("sustena_classify_sustain_id");
        String messageId = intent.getStringExtra("sustena_classify_message_id");
        if (sustainId != null && messageId != null) {
            SmsAuthStore.setPendingClassifyTarget(getApplicationContext(), sustainId, messageId);
        }
    }
}
