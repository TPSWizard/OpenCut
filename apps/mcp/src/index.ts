#!/usr/bin/env bun
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { createOpenCutMcpServer } from "./server.ts";

const { server, runner } = createOpenCutMcpServer();

const shutdown = async () => {
	await runner.stop().catch(() => {});
	process.exit(0);
};
process.on("SIGINT", () => void shutdown());
process.on("SIGTERM", () => void shutdown());

const transport = new StdioServerTransport();
// When the client goes away (stdin closes), release the browser too.
transport.onclose = () => void shutdown();
process.stdin.on("end", () => void shutdown());
await server.connect(transport);
console.error("opencut-mcp: ready on stdio");
