import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";
import { prettyJSON } from "hono/pretty-json";
import { formatDate } from "@{{projectName}}/utils";

const app = new Hono();

// Middleware
app.use("*", logger());
app.use("*", prettyJSON());
app.use(
  "*",
  cors({
    origin: ["http://localhost:3000"],
    credentials: true,
  })
);

// Routes
app.get("/", (c) => {
  return c.json({
    name: "{{projectName}} API",
    version: "0.1.0",
    timestamp: formatDate(new Date()),
  });
});

app.get("/health", (c) => {
  return c.json({
    status: "ok",
    uptime: process.uptime(),
  });
});

app.get("/api/hello", (c) => {
  const name = c.req.query("name") || "World";
  return c.json({
    message: `Hello, ${name}!`,
    timestamp: formatDate(new Date()),
  });
});

// 404 handler
app.notFound((c) => {
  return c.json({ error: "Not Found" }, 404);
});

// Error handler
app.onError((err, c) => {
  console.error(err);
  return c.json({ error: "Internal Server Error" }, 500);
});

// Start server
const port = Number(process.env.PORT) || 4000;

console.log(`🚀 API running at http://localhost:${port}`);

export default {
  port,
  fetch: app.fetch,
};
