# Open in ConzaCut

How another app (the Overtime studio, a script, a server) hands a project
to ConzaCut. Every route ends in an ordinary project bundle
(`.opencut.zip`, see `src/project/bundle.ts`), and nothing is added until
the person confirms.

## A link to a hosted bundle

```
https://conzacut.conzahub.com/open?bundle=<URL-encoded bundle URL>
```

- The bundle URL must be `https://` (`http://` only on localhost,
  127.0.0.1 or [::1]), without credentials in it. Signed URLs with query
  strings are fine once encoded (`encodeURIComponent`).
- The page shows the host and asks before downloading, then shows the
  project's name, size, length and media count and asks again before
  adding it.
- The bundle's server must let the app download it: respond to `GET` with
  `Access-Control-Allow-Origin: https://conzacut.conzahub.com` (or `*`).
- Up to 2 GB in the browser, 8 GB on the desktop.
- `conzacut://open?bundle=<URL>` means the same and is what the desktop
  editor accepts on its command line (`opencut-desktop "<link>"`); the
  desktop's projects page also has **Open copied link**. The desktop does
  not register itself for `conzacut://` links.

`openLinkFor({ bundleUrl, origin })` in `src/project/open-link.ts` builds the
link; `bundleUrlFromLink` (and the desktop's `editor::bundle_url_from_link`)
read it, by the rules tested in `fixtures/open-links/cases.json`.

## Handing over a bundle from a page (no hosting)

```js
const tab = window.open("https://conzacut.conzahub.com/open?handoff=1");
window.addEventListener("message", (event) => {
  if (event.source !== tab || event.data?.type !== "conzacut:ready") return;
  tab.postMessage({ type: "conzacut:bundle", bundle: bundleBlob }, "https://conzacut.conzahub.com");
});
```

The page only accepts a message from the window that opened it, shows that
window's origin with the summary, and asks before adding the project.

## From a script or the MCP server

`project.openLink { link }` downloads and adds the bundle a link points at
(Editor API; same rules, no confirmation, since the caller is the user's
own script).
