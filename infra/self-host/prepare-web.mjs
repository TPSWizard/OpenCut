// Makes the web build's generated wrangler config fit for self-hosting:
// drops the custom-domain route (it names the hosted site) so the local
// runtime serves whatever host the browser asked for.
//   node prepare-web.mjs <apps/web/dist/server/wrangler.json>
import { readFileSync, writeFileSync } from "node:fs";

const path = process.argv[2];
if (!path) throw new Error("usage: node prepare-web.mjs <dist/server/wrangler.json>");
const config = JSON.parse(readFileSync(path, "utf8"));
delete config.routes;
delete config.route;
config.name = "opencut-web";
writeFileSync(path, JSON.stringify(config));
console.log(`prepared ${path}: ${config.name}, assets in ${config.assets?.directory}`);
