package online.vyybandasky.sustena;

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
    }
}
