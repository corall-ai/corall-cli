import assert from "node:assert/strict";
import fs from "node:fs/promises";
import http, {} from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createPollingService } from "../src/service.js";
test("polling service forwards hook payload and acks event", async () => {
    const hookPayload = {
        message: "You have a new order",
        name: "Corall",
        sessionKey: "hook:corall:order-1",
        deliver: false,
    };
    const forwardedHooks = [];
    const acks = [];
    let pollCount = 0;
    const hookServer = http.createServer(async (req, res) => {
        assert.equal(req.method, "POST");
        assert.equal(req.url, "/hooks/agent");
        assert.equal(req.headers.authorization, "Bearer hook-token");
        forwardedHooks.push(await readJson(req));
        sendJson(res, 200, { ok: true });
    });
    const eventbusServer = http.createServer(async (req, res) => {
        assert.equal(req.headers.authorization, "Bearer agent-token");
        const requestUrl = req.url;
        assert.equal(typeof requestUrl, "string");
        if (typeof requestUrl !== "string") {
            throw new Error("request URL is missing");
        }
        const url = new URL(requestUrl, "http://127.0.0.1");
        if (req.method === "GET") {
            assert.equal(url.pathname, "/v1/agents/agent-1/events");
            assert.equal(url.searchParams.get("consumerId"), "test-consumer");
            pollCount += 1;
            sendJson(res, 200, {
                events: pollCount === 1 ? [{ id: "stream-1", hook: hookPayload }] : [],
            });
            return;
        }
        assert.equal(req.method, "POST");
        assert.equal(url.pathname, "/v1/agents/agent-1/events/stream-1/ack");
        acks.push("stream-1");
        sendJson(res, 200, { ok: true });
    });
    await listen(hookServer);
    await listen(eventbusServer);
    const eventbusUrl = serverUrl(eventbusServer);
    const hookUrl = `${serverUrl(hookServer)}/hooks/agent`;
    const service = createPollingService({
        api: testApi(),
        config: {
            baseUrl: eventbusUrl,
            agentId: "agent-1",
            agentToken: "agent-token",
            credentialProfile: "provider",
            consumerId: "test-consumer",
            hookUrl,
            waitSeconds: 0,
            requestTimeoutMs: 500,
            ackTimeoutMs: 500,
            idleDelayMs: 5,
            errorBackoffMs: 5,
            maxErrorBackoffMs: 10,
            recentEventTtlMs: 1_000,
        },
    });
    try {
        await service.start();
        await waitFor(() => forwardedHooks.length === 1 && acks.length === 1);
        assert.deepEqual(forwardedHooks[0], hookPayload);
        assert.deepEqual(acks, ["stream-1"]);
    }
    finally {
        await service.stop();
        await close(eventbusServer);
        await close(hookServer);
    }
});
test("polling service deduplicates repeated delivery while acking every stream id", async () => {
    const hookPayload = {
        message: "You have a duplicated order",
        name: "Corall",
        sessionKey: "hook:corall:order-duplicate",
        deliver: false,
    };
    const forwardedHooks = [];
    const acks = [];
    let pollCount = 0;
    const hookServer = http.createServer(async (req, res) => {
        forwardedHooks.push(await readJson(req));
        sendJson(res, 200, { ok: true });
    });
    const eventbusServer = http.createServer((req, res) => {
        const requestUrl = req.url;
        assert.equal(typeof requestUrl, "string");
        if (typeof requestUrl !== "string") {
            throw new Error("request URL is missing");
        }
        const url = new URL(requestUrl, "http://127.0.0.1");
        if (req.method === "GET") {
            pollCount += 1;
            sendJson(res, 200, {
                events: pollCount === 1
                    ? [
                        { id: "stream-1", eventId: "order.paid:order-duplicate", hook: hookPayload },
                        { id: "stream-2", eventId: "order.paid:order-duplicate", hook: hookPayload },
                    ]
                    : [],
            });
            return;
        }
        assert.equal(req.method, "POST");
        acks.push(url.pathname);
        sendJson(res, 200, { ok: true });
    });
    await listen(hookServer);
    await listen(eventbusServer);
    const service = createPollingService({
        api: testApi(),
        config: {
            baseUrl: serverUrl(eventbusServer),
            agentId: "agent-1",
            agentToken: "agent-token",
            credentialProfile: "provider",
            consumerId: "test-consumer",
            hookUrl: `${serverUrl(hookServer)}/hooks/agent`,
            waitSeconds: 0,
            requestTimeoutMs: 500,
            ackTimeoutMs: 500,
            idleDelayMs: 5,
            errorBackoffMs: 5,
            maxErrorBackoffMs: 10,
            recentEventTtlMs: 1_000,
        },
    });
    try {
        await service.start();
        await waitFor(() => forwardedHooks.length === 1 && acks.length === 2);
        assert.deepEqual(forwardedHooks[0], hookPayload);
        assert.deepEqual(acks, [
            "/v1/agents/agent-1/events/stream-1/ack",
            "/v1/agents/agent-1/events/stream-2/ack",
        ]);
    }
    finally {
        await service.stop();
        await close(eventbusServer);
        await close(hookServer);
    }
});
test("polling service recovers after poll request failure", async () => {
    const hookPayload = {
        message: "You have a recovered order",
        name: "Corall",
        sessionKey: "hook:corall:order-recovered",
        deliver: false,
    };
    const forwardedHooks = [];
    const acks = [];
    let pollCount = 0;
    const hookServer = http.createServer(async (req, res) => {
        forwardedHooks.push(await readJson(req));
        sendJson(res, 200, { ok: true });
    });
    const eventbusServer = http.createServer((req, res) => {
        const requestUrl = req.url;
        assert.equal(typeof requestUrl, "string");
        if (typeof requestUrl !== "string") {
            throw new Error("request URL is missing");
        }
        const url = new URL(requestUrl, "http://127.0.0.1");
        if (req.method === "GET") {
            pollCount += 1;
            if (pollCount === 1) {
                sendJson(res, 503, { error: "temporary redis failure" });
                return;
            }
            sendJson(res, 200, {
                events: pollCount === 2
                    ? [{ id: "stream-recovered", eventId: "order.paid:order-recovered", hook: hookPayload }]
                    : [],
            });
            return;
        }
        assert.equal(req.method, "POST");
        acks.push(url.pathname);
        sendJson(res, 200, { ok: true });
    });
    await listen(hookServer);
    await listen(eventbusServer);
    const service = createPollingService({
        api: testApi(),
        config: {
            baseUrl: serverUrl(eventbusServer),
            agentId: "agent-1",
            agentToken: "agent-token",
            credentialProfile: "provider",
            consumerId: "test-consumer",
            hookUrl: `${serverUrl(hookServer)}/hooks/agent`,
            waitSeconds: 0,
            requestTimeoutMs: 500,
            ackTimeoutMs: 500,
            idleDelayMs: 5,
            errorBackoffMs: 5,
            maxErrorBackoffMs: 10,
            recentEventTtlMs: 1_000,
        },
    });
    try {
        await service.start();
        await waitFor(() => forwardedHooks.length === 1 && acks.length === 1);
        assert.equal(pollCount >= 2, true);
        assert.deepEqual(forwardedHooks[0], hookPayload);
        assert.deepEqual(acks, ["/v1/agents/agent-1/events/stream-recovered/ack"]);
    }
    finally {
        await service.stop();
        await close(eventbusServer);
        await close(hookServer);
    }
});
test("polling service does not forward again when ack fails after hook delivery", async () => {
    const hookPayload = {
        message: "You have an ack retry order",
        name: "Corall",
        sessionKey: "hook:corall:order-ack-retry",
        deliver: false,
    };
    const forwardedHooks = [];
    let ackAttempts = 0;
    const hookServer = http.createServer(async (req, res) => {
        forwardedHooks.push(await readJson(req));
        sendJson(res, 200, { ok: true });
    });
    const eventbusServer = http.createServer((req, res) => {
        const requestUrl = req.url;
        assert.equal(typeof requestUrl, "string");
        if (typeof requestUrl !== "string") {
            throw new Error("request URL is missing");
        }
        if (req.method === "GET") {
            sendJson(res, 200, {
                events: ackAttempts < 2
                    ? [{ id: "stream-ack-retry", eventId: "order.paid:order-ack-retry", hook: hookPayload }]
                    : [],
            });
            return;
        }
        assert.equal(req.method, "POST");
        ackAttempts += 1;
        if (ackAttempts === 1) {
            sendJson(res, 503, { error: "temporary ack failure" });
            return;
        }
        sendJson(res, 200, { ok: true });
    });
    await listen(hookServer);
    await listen(eventbusServer);
    const service = createPollingService({
        api: testApi(),
        config: {
            baseUrl: serverUrl(eventbusServer),
            agentId: "agent-1",
            agentToken: "agent-token",
            credentialProfile: "provider",
            consumerId: "test-consumer",
            hookUrl: `${serverUrl(hookServer)}/hooks/agent`,
            waitSeconds: 0,
            requestTimeoutMs: 500,
            ackTimeoutMs: 500,
            idleDelayMs: 5,
            errorBackoffMs: 5,
            maxErrorBackoffMs: 10,
            recentEventTtlMs: 1_000,
        },
    });
    try {
        await service.start();
        await waitFor(() => forwardedHooks.length === 1 && ackAttempts === 2);
        assert.deepEqual(forwardedHooks[0], hookPayload);
    }
    finally {
        await service.stop();
        await close(eventbusServer);
        await close(hookServer);
    }
});
test("polling service starts after credentials add agent id", async () => {
    const homeDir = await fs.mkdtemp(path.join(os.tmpdir(), "corall-polling-home-"));
    const previousHome = process.env.HOME;
    const profile = "late-provider";
    const hookPayload = {
        message: "You have a new order",
        name: "Corall",
        sessionKey: "hook:corall:order-late",
        deliver: false,
    };
    const forwardedHooks = [];
    const acks = [];
    let pollCount = 0;
    process.env.HOME = homeDir;
    const hookServer = http.createServer(async (req, res) => {
        assert.equal(req.method, "POST");
        assert.equal(req.url, "/hooks/agent");
        assert.equal(req.headers.authorization, "Bearer hook-token");
        forwardedHooks.push(await readJson(req));
        sendJson(res, 200, { ok: true });
    });
    const eventbusServer = http.createServer(async (req, res) => {
        assert.equal(req.headers.authorization, "Bearer agent-token");
        const requestUrl = req.url;
        assert.equal(typeof requestUrl, "string");
        if (typeof requestUrl !== "string") {
            throw new Error("request URL is missing");
        }
        const url = new URL(requestUrl, "http://127.0.0.1");
        if (req.method === "GET") {
            assert.equal(url.pathname, "/v1/agents/agent-late/events");
            assert.match(url.searchParams.get("consumerId") ?? "", /^corall-polling:agent-late:/);
            pollCount += 1;
            sendJson(res, 200, {
                events: pollCount === 1 ? [{ id: "stream-late", hook: hookPayload }] : [],
            });
            return;
        }
        assert.equal(req.method, "POST");
        assert.equal(url.pathname, "/v1/agents/agent-late/events/stream-late/ack");
        acks.push("stream-late");
        sendJson(res, 200, { ok: true });
    });
    await listen(hookServer);
    await listen(eventbusServer);
    const eventbusUrl = serverUrl(eventbusServer);
    const hookUrl = `${serverUrl(hookServer)}/hooks/agent`;
    const service = createPollingService({
        api: testApi(),
        config: {
            baseUrl: eventbusUrl,
            agentId: undefined,
            agentToken: "agent-token",
            credentialProfile: profile,
            consumerId: undefined,
            hookUrl,
            waitSeconds: 0,
            requestTimeoutMs: 500,
            ackTimeoutMs: 500,
            idleDelayMs: 5,
            errorBackoffMs: 5,
            maxErrorBackoffMs: 10,
            recentEventTtlMs: 1_000,
        },
    });
    try {
        await service.start();
        await delayMs(30);
        assert.equal(pollCount, 0);
        const credentialsDir = path.join(homeDir, ".corall", "credentials");
        await fs.mkdir(credentialsDir, { recursive: true });
        await fs.writeFile(path.join(credentialsDir, `${profile}.json`), JSON.stringify({ agentId: "agent-late" }));
        await waitFor(() => forwardedHooks.length === 1 && acks.length === 1, 2_000);
        assert.deepEqual(forwardedHooks[0], hookPayload);
        assert.deepEqual(acks, ["stream-late"]);
    }
    finally {
        await service.stop();
        await close(eventbusServer);
        await close(hookServer);
        if (previousHome === undefined) {
            delete process.env.HOME;
        }
        else {
            process.env.HOME = previousHome;
        }
        await fs.rm(homeDir, { recursive: true, force: true });
    }
});
test("polling service restarts when credential agent id changes", async () => {
    const homeDir = await fs.mkdtemp(path.join(os.tmpdir(), "corall-polling-home-"));
    const previousHome = process.env.HOME;
    const profile = "switch-provider";
    const hookPayload = {
        message: "You have a switched order",
        name: "Corall",
        sessionKey: "hook:corall:order-switch",
        deliver: false,
    };
    const forwardedHooks = [];
    const acks = [];
    const polledPaths = [];
    let sentSwitchedEvent = false;
    process.env.HOME = homeDir;
    await writeCredential(homeDir, profile, "agent-one");
    const hookServer = http.createServer(async (req, res) => {
        assert.equal(req.method, "POST");
        forwardedHooks.push(await readJson(req));
        sendJson(res, 200, { ok: true });
    });
    const eventbusServer = http.createServer((req, res) => {
        const requestUrl = req.url;
        assert.equal(typeof requestUrl, "string");
        if (typeof requestUrl !== "string") {
            throw new Error("request URL is missing");
        }
        const url = new URL(requestUrl, "http://127.0.0.1");
        if (req.method === "GET") {
            polledPaths.push(url.pathname);
            if (url.pathname === "/v1/agents/agent-two/events" && !sentSwitchedEvent) {
                sentSwitchedEvent = true;
                sendJson(res, 200, { events: [{ id: "stream-two", hook: hookPayload }] });
                return;
            }
            sendJson(res, 200, { events: [] });
            return;
        }
        assert.equal(req.method, "POST");
        assert.equal(url.pathname, "/v1/agents/agent-two/events/stream-two/ack");
        acks.push("stream-two");
        sendJson(res, 200, { ok: true });
    });
    await listen(hookServer);
    await listen(eventbusServer);
    const service = createPollingService({
        api: testApi(),
        config: {
            baseUrl: serverUrl(eventbusServer),
            agentId: undefined,
            agentToken: "agent-token",
            credentialProfile: profile,
            consumerId: undefined,
            hookUrl: `${serverUrl(hookServer)}/hooks/agent`,
            waitSeconds: 0,
            requestTimeoutMs: 500,
            ackTimeoutMs: 500,
            idleDelayMs: 5,
            errorBackoffMs: 5,
            maxErrorBackoffMs: 10,
            recentEventTtlMs: 1_000,
        },
    });
    try {
        await service.start();
        await waitFor(() => polledPaths.includes("/v1/agents/agent-one/events"), 1_000);
        await writeCredential(homeDir, profile, "agent-two");
        await waitFor(() => forwardedHooks.length === 1 && acks.length === 1, 2_000);
        assert.deepEqual(forwardedHooks[0], hookPayload);
        assert.deepEqual(acks, ["stream-two"]);
        assert.ok(polledPaths.includes("/v1/agents/agent-two/events"));
    }
    finally {
        await service.stop();
        await close(eventbusServer);
        await close(hookServer);
        if (previousHome === undefined) {
            delete process.env.HOME;
        }
        else {
            process.env.HOME = previousHome;
        }
        await fs.rm(homeDir, { recursive: true, force: true });
    }
});
function testApi() {
    return {
        config: {
            hooks: { token: "hook-token" },
            gateway: { port: 18789 },
        },
        logger: {
            debug(_message) { },
            info(_message) { },
            warn(_message) { },
            error(_message) { },
        },
    };
}
function listen(server) {
    return new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(0, "127.0.0.1", () => {
            server.off("error", reject);
            resolve();
        });
    });
}
function close(server) {
    return new Promise((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
    });
}
function serverUrl(server) {
    const address = server.address();
    assert.equal(typeof address, "object");
    assert.notEqual(address, null);
    const info = address;
    return `http://${info.address}:${info.port}`;
}
async function readJson(req) {
    const chunks = [];
    for await (const chunk of req) {
        chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk)));
    }
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}
function sendJson(res, status, body) {
    res.writeHead(status, { "content-type": "application/json" });
    res.end(JSON.stringify(body));
}
async function delayMs(ms) {
    await new Promise((resolve) => {
        setTimeout(resolve, ms);
    });
}
async function writeCredential(homeDir, profile, agentId) {
    const credentialsDir = path.join(homeDir, ".corall", "credentials");
    await fs.mkdir(credentialsDir, { recursive: true });
    await fs.writeFile(path.join(credentialsDir, `${profile}.json`), JSON.stringify({ agentId }));
}
async function waitFor(predicate, timeoutMs = 1_000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (predicate()) {
            return;
        }
        await new Promise((resolve) => {
            setTimeout(resolve, 10);
        });
    }
    throw new Error("timed out waiting for predicate");
}
//# sourceMappingURL=service.test.js.map