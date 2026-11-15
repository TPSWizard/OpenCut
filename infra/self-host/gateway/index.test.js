import { describe, expect, it } from "bun:test";
import gateway, { apiTarget, publicOrigin } from "./index.js";

const request = (url, headers = {}) => new Request(url, { headers });

describe("self-host gateway", () => {
  it("sends /api to the API worker without the prefix", () => {
    expect(apiTarget(request("http://localhost:8080/api/health"))).toBe("http://localhost:8080/health");
    expect(apiTarget(request("http://localhost:8080/api/sounds/search?q=rain&page=2"))).toBe(
      "http://localhost:8080/sounds/search?q=rain&page=2",
    );
    expect(apiTarget(request("http://localhost:8080/api"))).toBe("http://localhost:8080/");
    for (const path of ["/", "/editor/abc", "/apiary", "/assets/api/x.js"]) {
      expect(apiTarget(request(`http://localhost:8080${path}`))).toBeNull();
    }
  });

  it("uses the reverse proxy's public origin when it sends one", () => {
    const behind = request("http://conzacut:8080/api/share/complete", {
      "X-Forwarded-Proto": "https",
      "X-Forwarded-Host": "cut.example.com",
    });
    expect(publicOrigin(behind)).toBe("https://cut.example.com");
    expect(apiTarget(behind)).toBe("https://cut.example.com/share/complete");
    expect(publicOrigin(request("http://localhost:8080/", { "X-Forwarded-Host": "[::1]:8443" }))).toBe("http://[::1]:8443");
    // Junk falls back to the request's own origin.
    const junk = request("http://localhost:8080/", { "X-Forwarded-Proto": "ftp", "X-Forwarded-Host": "evil.example/path" });
    expect(publicOrigin(junk)).toBe("http://localhost:8080");
  });

  it("forwards bodies and marks the prefix", async () => {
    const seen = [];
    const env = {
      WEB: { fetch: async (req) => (seen.push(["web", req.url]), new Response("web")) },
      API: {
        fetch: async (req) => {
          seen.push(["api", req.url, req.method, req.headers.get("X-Forwarded-Prefix"), await req.text()]);
          return new Response("api");
        },
      },
    };
    await gateway.fetch(new Request("http://localhost:8080/editor"), env);
    await gateway.fetch(
      new Request("http://localhost:8080/api/feedback", {
        method: "POST",
        body: "hello",
        headers: { "X-Forwarded-Prefix": "/spoofed" },
      }),
      env,
    );
    expect(seen).toEqual([
      ["web", "http://localhost:8080/editor"],
      ["api", "http://localhost:8080/feedback", "POST", "/api", "hello"],
    ]);
  });
});
