package com.rendivs925.cosmicsystems;

import com.google.androidgamesdk.GameActivity;

/**
 * GameActivity shell for the Rust simulator.
 *
 * <p>GameActivity reads {@code android.app.lib_name} from the manifest and
 * loads that cdylib itself. The explicit {@code loadLibrary} here makes the
 * dependency obvious and guarantees the JNI symbols are present before
 * {@code onCreate} runs.
 */
public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("cosmic_systems_wasm");
    }
}
