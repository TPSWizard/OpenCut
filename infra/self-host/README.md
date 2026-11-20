# Self-hosting ConzaCut

This folder builds one container image that runs the web editor and its API
worker on your own machine, behind a single port:

```
browser ──► :8080  gateway worker ──► /api/*  API worker (apps/api)
                                  └─► else    web app (apps/web build)
```

All three are Cloudflare Workers running together in one `wrangler dev`
session on workerd, the open-source Workers runtime. The code is the same as
the hosted site. The web app is built to call its API at `/api` on its own
origin, so one image works on any host name without being rebuilt.

**It is not a render server.** The editor still decodes, previews and exports
video in the browser, as it does on the hosted site. For rendering on a
machine, use the desktop app or `opencut-render` (see `apps/desktop`).

## Quick start

You need Docker with Compose v2.24 or newer. The first build compiles
`wasm-pack` and then the Rust core to WebAssembly, which takes several
minutes; later builds reuse that step unless `crates/` changed. The image is
about 830 MB.

```sh
cd infra/self-host
cp .env.example .env
docker compose up -d --build
```

Open http://localhost:8080. To check that it is up:

```sh
curl http://localhost:8080/api/health
```

To run without Compose, from the repository root:

```sh
docker build -f infra/self-host/Dockerfile -t conzacut .
docker run -d -p 127.0.0.1:8080:8080 -v conzacut-data:/data \
  -e SHARE_UPLOAD_KEY=change-me conzacut
```

## Using it from other computers

Browsers only allow the editor's video encoding (WebCodecs), its offline
storage and its service worker on `localhost` or over HTTPS. Plain
`http://192.168.x.x:8080` opens the page, but exporting and saving will not
work. For this reason Compose publishes the port on `127.0.0.1` only.

To serve it on a domain with HTTPS:

1. Point the domain's DNS at this machine and open ports 80 and 443.
2. Set `CONZACUT_DOMAIN=cut.example.com` in `.env`.
3. Run `docker compose --profile https up -d --build`.

Caddy gets a certificate from Let's Encrypt and proxies to the container.

You can use your own reverse proxy instead. It must pass the `Host` header,
or send `X-Forwarded-Proto` and `X-Forwarded-Host`, so that share links point
at your domain. It must also allow request bodies of 32 MB, the size of one
share upload part. A Cloudflare Tunnel to `http://localhost:8080` also works.

## Settings

Every setting is an environment variable, normally set in `.env`. All of
them are optional. A feature whose setting is missing is switched off, and
the editor says so where you would use it.

| Variable | What it turns on |
| --- | --- |
| `SHARE_UPLOAD_KEY` | Sharing projects by link. Uploading a share needs this passcode; opening one needs only the link. Use a long random value. |
| `FREESOUND_API_KEY` | Sound search through Freesound ([get a key](https://freesound.org/apiv2/apply)). The 663 bundled sound effects work without one. |
| `PIXABAY_API_KEY` | Stock images and video from Pixabay ([API docs](https://pixabay.com/api/docs/)). Openverse images work without a key. |
| `SEGMIND_API_KEY` | Face swap through Segmind's hosted HyperSwap. Each swap spends your Segmind credit, so it also needs `SHARE_UPLOAD_KEY`. Segmind fetches the clip from this server's `/api/faceswap/files/` links, so the server must be reachable from the internet. |
| `ANTHROPIC_API_KEY` | Claude's search words for script-to-B-roll. Each use spends your API credit, so it also needs `SHARE_UPLOAD_KEY` (the editor sends the passcode). Without it the editor's own search words are used. |
| `CONZACUT_PORT` | The port on this computer (default `8080`). |
| `CONZACUT_DOMAIN` | Your domain, for the Caddy HTTPS service. |
| `LOG_LEVEL` | wrangler's log level: `debug`, `info` (default), `warn`, `error` or `none`. |

## Where things are stored

- **Projects and media stay in each person's browser** (origin private file
  system and IndexedDB), exactly as on the hosted site. They are not stored on
  the server, so moving to a new browser means exporting a project bundle.
- **Shares** are stored in the `conzacut-data` volume, mounted at `/data` as
  wrangler's local R2 storage. A share expires after 7 days. Expired shares
  are deleted whenever someone starts a new share, and when someone opens an
  expired link.

Back up the volume with any Docker volume backup method, for example:

```sh
docker run --rm -v conzacut-data:/data -v "$PWD":/backup busybox tar czf /backup/conzacut-data.tgz -C /data .
```

## The desktop app with your server

The desktop editor reads these from `~/.opencut/settings.json`:

```json
{
  "apiUrl": "https://cut.example.com/api",
  "webUrl": "https://cut.example.com",
  "shareKey": "the SHARE_UPLOAD_KEY passcode"
}
```

`apiUrl` is used for sharing and Pixabay stock. `webUrl` is the site that
share links open in. The environment variables `OPENCUT_API_URL`,
`OPENCUT_WEB_URL` and `OPENCUT_SHARE_KEY` override the file.

## Updating

```sh
git pull
docker compose up -d --build
```

The volume is kept, so shares survive updates.

## What is different from the hosted site

- **The runtime.** The container uses wrangler's local mode, which is made
  for development. It is fine for a home server or a team, but it is not
  Cloudflare's network: there is no edge cache and no DDoS protection. The
  local storage explorer and trace viewer that wrangler normally serves under
  `/cdn-cgi/` are switched off (`X_LOCAL_EXPLORER=false`,
  `X_LOCAL_OBSERVABILITY=false`).
- **No rate limiting.** If you set a Freesound or Pixabay key and put the
  server on the internet, anyone who can reach it can use your key's quota.
  Keep it private, or put authentication in front (for example Cloudflare
  Access, or `basic_auth` in the Caddyfile).
- **No feedback inbox.** Sending feedback from the editor shows an error,
  because the container has nowhere to deliver it.
- **Other services the browser still calls.** The fonts list (Fontsource)
  and font files (jsDelivr), icon stickers (Iconify), Openverse images and
  Freesound previews are fetched from those services. Speech transcription and translation download
  their models from Hugging Face the first time they are used.

## How it is put together

| File | Role |
| --- | --- |
| `Dockerfile` | Three stages: Rust to wasm (`crates/wasm`), the web build with `VITE_API_URL=/api` plus the API worker's dependencies, then a Node runtime that runs wrangler as an unprivileged user. |
| `gateway/index.js`, `gateway.jsonc` | The only worker with a port. Sends `/api/*` to the API worker without the prefix and adds `X-Forwarded-Prefix: /api`, so the API builds share URLs under `/api`. |
| `api/wrangler.jsonc` | The API worker's config for this container: local R2 for shares, and `PRUNE_EXPIRED_SHARES`. |
| `prepare-web.mjs` | Removes the hosted site's custom-domain route from the web build's generated config. |
| `entrypoint.sh` | Writes the API's secrets from the environment to `api/.dev.vars`, then starts the three workers in one wrangler session with state in `/data`. |
| `compose.yaml`, `Caddyfile`, `.env.example` | The Compose setup, with optional HTTPS. |
| `smoke-test.sh` | Checks a running server: pages, `/api/health`, a share round trip, and that `/cdn-cgi/` is closed. `bash smoke-test.sh http://localhost:8080 <passcode>` |

The gateway's tests run with the API worker's (`moon run api:test`, or
`bun run test` in `apps/api`). The `Self-host image` workflow builds the image
and runs the smoke test when these files change.
