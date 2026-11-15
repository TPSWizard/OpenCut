# ConzaCut for Android

The web editor (https://conzacut.conzahub.com) as an Android app: a
[Trusted Web Activity](https://developer.chrome.com/docs/android/trusted-web-activity)
that opens the site full screen in the phone's Chrome, with the phone editor
layout, offline support and everything else the web app has. There is no app
code; the launcher, share handling and notification delegation come from
[androidbrowserhelper](https://github.com/GoogleChrome/android-browser-helper)
(Apache-2.0).

- **Package:** `com.conzahub.conzacut`, minimum Android 7.0 (API 24), target API 36.
- **Opens:** `/projects`; links to `conzacut.conzahub.com` open in the app.
- **Share sheet:** videos, photos and audio shared from other apps go to the
  web app's share target (`/share-target`, see `apps/web/src/pwa`), which
  makes a "Shared clips" project with them.
- **No address bar:** Chrome hides it once the site's
  `apps/web/public/.well-known/assetlinks.json` lists the SHA-256 of the key
  that signed the app. If the APK and the file disagree, the app still works
  but shows the address bar.

## Building

GitHub Actions builds it (`.github/workflows/android.yml`) whenever this
folder changes, or by hand (Actions → Android app → Run workflow). Download
`conzacut-android-<n>` from the run: `app-release.apk` to install on a phone,
`app-release.aab` for Google Play.

Locally (needs the Android SDK and JDK 17+):

```bash
gradle -p apps/android assembleRelease
```

Without `ANDROID_KEYSTORE_PATH` and `ANDROID_KEYSTORE_PASSWORD` the release
build is signed with the debug key.

## The signing key

The release key (alias `conzacut`) lives with its owner, outside the repo,
and in the repository secrets `ANDROID_KEYSTORE_BASE64` and
`ANDROID_KEYSTORE_PASSWORD`. Every update has to be signed with the same
key, so keep a backup. A fork that builds with its own key must put its own
fingerprint in `assetlinks.json` (and use its own package name and site).
