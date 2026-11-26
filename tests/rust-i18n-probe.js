import GLib from 'gi://GLib';
import {run} from './common/process.js';

export async function probe() {
    const directory = GLib.getenv('CLIVUU_TEST_DIR');
    const results = [];
    try {
        await run(['/usr/bin/env', '-u', 'CLIVUU_LANGUAGE', GLib.getenv('CLIVUU_RUST_I18N_PROBE')],
            null, null, {GDK_BACKEND: 'wayland'});
        const report = JSON.parse(new TextDecoder().decode(
            GLib.file_get_contents(`${directory}/i18n-report.json`)[1]));
        for (const stage of report)
            results.push({name: `${stage.stage}: settings and shared editor localization`, passed: true, ...stage});
    } catch (error) {
        results.push({name: 'settings and shared editor localization across registered locales',
            passed: false, error: String(error)});
    }
    GLib.file_set_contents(`${directory}/results.json`, JSON.stringify(results));
}
