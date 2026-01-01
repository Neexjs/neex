import express from "express";
import cors from "cors";
import { formatDate } from "@{{projectName}}/utils";

const app = express();
const port = Number(process.env.PORT) || 4000;

// Middleware
app.use(cors({ origin: "http://localhost:3000", credentials: true }));
app.use(express.json());

// Request logger
app.use((req, _res, next) => {
  console.log(`${req.method} ${req.path}`);
  next();
});

// Routes
app.get("/", (_req, res) => {
  res.json({
    name: "{{projectName}} API",
    version: "0.1.0",
    timestamp: formatDate(new Date()),
  });
});

app.get("/health", (_req, res) => {
  res.json({
    status: "ok",
    uptime: process.uptime(),
  });
});

app.get("/api/hello", (req, res) => {
  const name = (req.query.name as string) || "World";
  res.json({
    message: `Hello, ${name}!`,
    timestamp: formatDate(new Date()),
  });
});

// 404 handler
app.use((_req, res) => {
  res.status(404).json({ error: "Not Found" });
});

// Error handler
app.use(
  (
    err: Error,
    _req: express.Request,
    res: express.Response,
    _next: express.NextFunction
  ) => {
    console.error(err);
    res.status(500).json({ error: "Internal Server Error" });
  }
);

// Start server
app.listen(port, () => {
  console.log(`🚀 API running at http://localhost:${port}`);
});
