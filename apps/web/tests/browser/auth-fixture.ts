import { execFileSync, spawn } from "node:child_process";
import { createHash, generateKeyPairSync, randomBytes, verify } from "node:crypto";
import { once } from "node:events";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { request, type IncomingMessage, type ServerResponse } from "node:http";
import { createServer } from "node:https";
import { createServer as createSocket, type AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test as base } from "@playwright/test";

const pk = "pk_lockdocs_fixture";
const sk = "sk_lockdocs_fixture";
const instance = "lockdocs-fixture";
export const email = "reader@example.test";
export const code = "012345";
const random = () => randomBytes(32).toString("base64url");
const hash = (value: string) => createHash("sha256").update(value).digest();

async function startFixture(billing: boolean) {
  const tmp = mkdtempSync(join(tmpdir(), "lockdocs-browser-auth-"));
  const cert = join(tmp, "cert.pem");
  const key = join(tmp, "key.pem");
  const methods = { email_otp: false, passkey: false, password: true, sign_up: true };
  const social = ["google", "github"].map(provider => ({ provider, enabled: true, credentials: "shared" }));
  const calls: string[] = [];
  const shipments: string[] = [];
  const rejections: string[] = [];
  const ticketSignal: { is_new_user?: boolean } = {};
  const tickets = new Map<string, string>();
  const sessions = new Map<string, string>();
  const otpFlows = new Map<string, string>();
  const challenges = new Map<string, string>();
  const ticket = (address: string) => {
    const value = random();
    tickets.set(value, address);
    return value;
  };
  const { privateKey, publicKey } = generateKeyPairSync("ec", { namedCurve: "prime256v1" });
  const passkey = {
    credentialId: randomBytes(32).toString("base64"),
    isResidentCredential: true,
    signCount: 0,
    rpId: "localhost",
    privateKey: privateKey.export({ type: "pkcs8", format: "der" }).toString("base64"),
    userHandle: Buffer.from("fixture-user").toString("base64"),
  };
  let origin = "";
  let appPort = 0;
  let app: ReturnType<typeof spawn> | undefined;
  let server: ReturnType<typeof createServer> | undefined;

  function json(res: ServerResponse, body: object, status = 200) {
    res.writeHead(status, { "content-type": "application/json" }).end(JSON.stringify(body));
  }
  async function handle(req: IncomingMessage, res: ServerResponse) {
    const url = new URL(req.url ?? "/", origin);
    const path = url.pathname;
    if (!path.startsWith("/v1/")) {
      const upstream = request({ hostname: "127.0.0.1", port: appPort, path: req.url, method: req.method, headers: {
        ...req.headers, "x-forwarded-host": url.host, "x-forwarded-proto": "https",
      } }, upstream => {
        res.writeHead(upstream.statusCode ?? 502, upstream.headers);
        upstream.pipe(res);
      });
      upstream.on("error", () => res.writeHead(502).end());
      req.pipe(upstream);
      return;
    }
    calls.push(path);
    const expiry = Math.floor(Date.now() / 1000) + 3600;
    const rejectContract = (reason: string) => { rejections.push(path + ": " + reason); json(res, {}, 403); };
    const sessionToken = req.headers.authorization?.slice("Bearer ".length) ?? "";
    if (path === "/v1/client/session" || path === "/v1/client/sign-out") {
      const address = sessions.get(sessionToken);
      if (!address) return json(res, {}, 401);
      if (path.endsWith("/sign-out")) { sessions.delete(sessionToken); return json(res, {}); }
      return json(res, { session: { id: "session-fixture", instance_id: instance, expires_at: expiry }, user: {
        id: address === email ? "usr_reader" : "usr_new_reader", name: "Fixture reader", email: address,
      } });
    }
    if (path === "/v1/client/token" && req.method === "POST" && sessions.get(sessionToken) === email) {
      return json(res, { access_token: "money-fixture-token", expires_in: 60 });
    }
    if (path === "/v1/money/orgs/fixture/projects/lockdocs/envs/test/entitlement_grants:getMine" && req.method === "GET" &&
        sessionToken === "money-fixture-token" && req.headers["x-sylphx-key"] === pk &&
        url.searchParams.get("feature") === "lockdocs_pro_seat_year") {
      return json(res, { subject_id: "usr_reader", entitled: false });
    }
    let raw = "";
    for await (const part of req) raw += part;
    const body = raw ? JSON.parse(raw) : {};
    if (path === "/v1/client/tickets:redeem") {
      if (req.headers.authorization !== "Bearer " + sk || url.searchParams.has("publishable_key") || req.method !== "POST") {
        return rejectContract("ticket exchange requires the secret credential");
      }
      const address = tickets.get(body.ticket);
      if (!address) return json(res, {}, 401);
      tickets.delete(body.ticket);
      const token = "identity_org_session_" + random();
      sessions.set(token, address);
      return json(res, { session: { token, expires_at: expiry }, ...ticketSignal });
    }
    if (req.headers.authorization || url.searchParams.get("publishable_key") !== pk) {
      return rejectContract("client endpoint requires a publishable key, not a secret bearer");
    }
    switch (path) {
      case "/v1/client/config":
        if (req.method !== "GET") return rejectContract("config method");
        return json(res, { instance: { id: instance }, methods, social });
      case "/v1/client/oauth/google/start": {
        const target = new URL(url.searchParams.get("redirect_url") ?? "");
        if (req.method !== "GET" || target.origin !== origin || target.pathname !== "/api/auth/callback" ||
            url.searchParams.get("session_mode") !== "server") return rejectContract("expected server OAuth callback");
        target.searchParams.set("auth_ticket", ticket(email));
        res.writeHead(303, { location: target.href }).end();
        return;
      }
      case "/v1/client/continue": {
        if (!methods.email_otp) return rejectContract("OTP is disabled");
        if (req.method !== "POST" || body.method !== "email_otp" || body.session_mode !== "server" || typeof body.email !== "string") {
          return rejectContract("continue request shape");
        }
        const flow = random();
        otpFlows.set(flow, body.email);
        shipments.push(body.email);
        return json(res, { step: "email_otp", flow, expires_in: 600 });
      }
      case "/v1/client/continue/verify": {
        if (!methods.email_otp || req.method !== "POST") return rejectContract("verify request shape");
        const address = otpFlows.get(body.flow);
        if (!address || body.code !== code) return json(res, {}, 401);
        otpFlows.delete(body.flow);
        return json(res, { ticket: ticket(address) });
      }
      case "/v1/client/sign-in/passkey/begin": {
        if (!methods.passkey || req.method !== "POST" || body.rp_id !== "localhost" || body.origin !== origin || Object.hasOwn(body, "email")) {
          return rejectContract("passkey begin must be discoverable and bound to the correct RP and origin");
        }
        const challengeId = random();
        const challenge = random();
        challenges.set(challengeId, challenge);
        return json(res, { challenge_id: challengeId, challenge, rp_id: "localhost", allowed_credential_ids: [], expires_at: expiry });
      }
      case "/v1/client/sign-in/passkey/verify": {
        if (!methods.passkey || req.method !== "POST" || body.session_mode !== "server") return rejectContract("passkey verify request shape");
        const challenge = challenges.get(body.challenge_id);
        if (!challenge) return rejectContract("no outstanding challenge");
        challenges.delete(body.challenge_id);
        const client = Buffer.from(body.client_data_json, "base64url");
        const data = JSON.parse(client.toString());
        const auth = Buffer.from(body.authenticator_data, "base64url");
        if (data.type !== "webauthn.get" || data.origin !== origin || data.challenge !== challenge ||
            body.credential_id !== Buffer.from(passkey.credentialId, "base64").toString("base64url") ||
            body.user_handle !== Buffer.from(passkey.userHandle, "base64").toString("base64url") ||
            !auth.subarray(0, 32).equals(hash("localhost")) || (auth[32] & 5) !== 5 ||
            !verify("sha256", Buffer.concat([auth, hash(client.toString())]), publicKey, Buffer.from(body.signature, "base64url"))) {
          return rejectContract("invalid browser assertion");
        }
        return json(res, { ticket: ticket(email) });
      }
      default: return rejectContract("unexpected provider endpoint");
    }
  }

  async function close() {
    if (app && app.exitCode === null && app.signalCode === null) { const done = once(app, "exit"); app.kill(); await done; }
    if (server?.listening) await new Promise<void>(resolve => { server!.close(() => resolve()); server!.closeAllConnections(); });
    rmSync(tmp, { recursive: true, force: true });
  }
  try {
    execFileSync("openssl", ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
      "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost", "-keyout", key, "-out", cert], { stdio: "ignore" });
    server = createServer({ cert: readFileSync(cert), key: readFileSync(key) }, (req, res) => {
      void handle(req, res).catch(() => { rejections.push("unhandled fixture request"); res.writeHead(500).end(); });
    }).listen(0, "localhost");
    await once(server, "listening");
    origin = "https://localhost:" + (server.address() as AddressInfo).port;
    const socket = createSocket().listen(0, "127.0.0.1");
    await once(socket, "listening");
    appPort = (socket.address() as AddressInfo).port;
    await new Promise<void>(resolve => socket.close(() => resolve()));
    app = spawn(process.execPath, [".output/server/index.mjs"], { env: {
      PATH: process.env.PATH, NODE_EXTRA_CA_CERTS: cert, HOST: "127.0.0.1", PORT: String(appPort), LOCKDOCS_APP_ORIGIN: origin,
      SYLPHX_AUTH_URL: origin, SYLPHX_AUTH_ORGANIZATION_ID: instance, SYLPHX_PUBLISHABLE_KEY: pk, SYLPHX_AUTH_SECRET_KEY: sk,
      ...(billing ? {
        SYLPHX_MONEY_URL: origin, SYLPHX_MONEY_PARENT: "orgs/fixture/projects/lockdocs/envs/test", SYLPHX_MONEY_API_KEY: "fixture-only",
        LOCKDOCS_CCR_TERMS_TEXT: "Fixture terms", LOCKDOCS_CCR_TERMS_VERSION: "fixture", LOCKDOCS_GA4_MEASUREMENT_ID: "G-TEST123456",
      } : {}),
    }, stdio: ["ignore", "pipe", "inherit"] });
    await once(app.stdout!, "data");
    if (!(await fetch(`http://127.0.0.1:${appPort}/api/health`)).ok) throw new Error("Fixture app did not start");
    return { origin, methods, calls, rejections, shipments, passkey, ticketSignal, close };
  } catch (error) {
    await close();
    throw error;
  }
}

export const test = base.extend<{ billing: boolean; auth: Awaited<ReturnType<typeof startFixture>> }>({
  billing: [false, { option: true }],
  ignoreHTTPSErrors: true, // A per-test certificate; the production child validates it via NODE_EXTRA_CA_CERTS.
  auth: async ({ billing }, use) => {
    const fixture = await startFixture(billing);
    try { await use(fixture); }
    finally { await fixture.close(); }
  },
});
