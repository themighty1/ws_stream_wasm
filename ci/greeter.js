// Minimal WebSocket "greeter" server for the atomics regression test
// (tests/atomics_repro.rs).
//
// Unlike an echo server, this sends one binary frame immediately after the
// handshake completes, before the client writes anything. That exercises the
// "message arrives before WsStream is constructed" race: the frame must be
// buffered by the early onmessage handler, not dropped.
//
// Usage: node ci/greeter.js [port]   (default 3313)
const http = require("http");
const crypto = require("crypto");

const MAGIC = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const PORT = Number(process.argv[2] || 3313);
const SENTINEL = 0x51;

http
  .createServer((req, res) => {
    const key = req.headers["sec-websocket-key"];
    if (!key) {
      res.writeHead(400);
      res.end();
      return;
    }
    const accept = crypto
      .createHash("sha1")
      .update(key + MAGIC)
      .digest("base64");

    res.writeHead(101, {
      Upgrade: "websocket",
      Connection: "Upgrade",
      "Sec-WebSocket-Accept": accept,
    });
    res.flushHeaders();

    // Immediately push a single binary frame (server->client, unmasked).
    const payload = Buffer.from([SENTINEL]);
    req.socket.write(Buffer.concat([Buffer.from([0x82, payload.length]), payload]));

    // Keep the connection open so the client can observe the frame.
  })
  .listen(PORT, "127.0.0.1", () => {
    console.log(`greeter listening on 127.0.0.1:${PORT}, sentinel=0x${SENTINEL.toString(16)}`);
  });