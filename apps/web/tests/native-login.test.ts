import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { test } from "node:test";
import type { APIEvent } from "@solidjs/start/server";
import { readConsent } from "../src/lib/cli-consent";
import { sealGrant } from "../src/lib/cli-envelope";
import { POST as exchange } from "../src/routes/api/cli/exchange";

// Exercised on Linux by the web CI job, which builds the real branch binary.
const binary = process.env.LOCKDOCS_TEST_BINARY;
test("native login exchanges a loopback callback and preserves the installed licence on signature refusal", { skip: !binary, timeout: 15_000 }, async (t) => {
  const origin = "https://preview.example.test";
  const secret = "fixture-envelope-key";
  process.env.LOCKDOCS_APP_ORIGIN = origin;
  process.env.SYLPHX_AUTH_SECRET_KEY = secret;
  const config = await mkdtemp(join(tmpdir(), "lockdocs-login-"));
  t.after(() => rm(config, { recursive: true, force: true }));
  await mkdir(join(config, "lockdocs"));
  const installed = join(config, "lockdocs", "licence");
  await writeFile(installed, "existing licence fixture");
  let exchanged = 0;
  let exchangeStatus: number | undefined;
  const server = createServer(async (req, res) => {
    if (req.method !== "POST" || req.url !== "/api/cli/exchange" || req.headers.origin) {
      res.writeHead(404).end();
      return;
    }
    exchanged++;
    const parts: Buffer[] = [];
    for await (const part of req) parts.push(part);
    const reply = await exchange({ request: new Request(origin + req.url, {
      method: "POST", headers: { "content-type": req.headers["content-type"] ?? "" }, body: Buffer.concat(parts).toString(),
    }) } as APIEvent);
    exchangeStatus = reply.status;
    res.writeHead(reply.status, Object.fromEntries(reply.headers)).end(await reply.text());
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(() => { server.close(); server.closeAllConnections(); });
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  const cli = spawn(binary!, ["login", "--no-browser"], { env: {
    HOME: config, XDG_CONFIG_HOME: config, LOCKDOCS_WEB_URL: "http://127.0.0.1:" + address.port,
  } });
  t.after(() => { if (cli.exitCode === null) cli.kill(); });
  const exited = once(cli, "close");
  let stderr = "";
  cli.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
  const lines = createInterface({ input: cli.stdout });
  let authorise: URL | undefined;
  for await (const line of lines) {
    if (line.startsWith("http://")) { authorise = new URL(line); break; }
  }
  assert.equal(authorise?.pathname, "/cli/authorize");
  assert.equal(authorise?.origin, "http://127.0.0.1:" + address.port);
  const input = readConsent(Object.fromEntries(authorise!.searchParams));
  assert.ok(input);
  const callback = new URL("http://127.0.0.1:" + input.port + "/callback");
  callback.search = new URLSearchParams({ state: input.state, code: sealGrant(secret, origin, input, "synthetic-unsigned-grant") }).toString();
  assert.equal((await fetch(callback, { signal: AbortSignal.timeout(4_000) })).status, 200);
  const [status] = await exited;
  assert.equal(exchanged, 1);
  assert.equal(exchangeStatus, 200);
  assert.equal(status, 1);
  assert.match(stderr, /did not return a valid lockdocs licence/);
  assert.ok(!stderr.includes("synthetic-unsigned-grant"));
  assert.equal(await readFile(installed, "utf8"), "existing licence fixture");
});
