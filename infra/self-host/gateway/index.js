// Self-hosted ConzaCut serves the editor and its API from one origin:
// `/api/*` goes to the API worker with the prefix removed, everything else
// to the web app. Both are separate workers in the same local runtime,
// reached through service bindings (see ../gateway.jsonc).

const PREFIX = "/api";

/**
 * The origin the browser used. A reverse proxy that terminates TLS sends
 * X-Forwarded-Proto and X-Forwarded-Host; without one, the request's own.
 */
export function publicOrigin(request) {
  const url = new URL(request.url);
  const proto = request.headers.get("X-Forwarded-Proto")?.split(",")[0].trim().toLowerCase();
  const host = request.headers.get("X-Forwarded-Host")?.split(",")[0].trim();
  const scheme = proto === "http" || proto === "https" ? proto : url.protocol.slice(0, -1);
  const authority = host && /^[A-Za-z0-9.-]+(:[0-9]{1,5})?$|^\[[0-9A-Fa-f:.]+\](:[0-9]{1,5})?$/.test(host) ? host : url.host;
  return `${scheme}://${authority}`;
}

/** The API worker's URL for a request under `/api`, or null for the web app. */
export function apiTarget(request) {
  const url = new URL(request.url);
  if (url.pathname !== PREFIX && !url.pathname.startsWith(`${PREFIX}/`)) return null;
  return `${publicOrigin(request)}${url.pathname.slice(PREFIX.length) || "/"}${url.search}`;
}

export default {
  async fetch(request, env) {
    const target = apiTarget(request);
    if (!target) return env.WEB.fetch(request);
    const forwarded = new Request(target, request);
    forwarded.headers.set("X-Forwarded-Prefix", PREFIX);
    return env.API.fetch(forwarded);
  },
};
